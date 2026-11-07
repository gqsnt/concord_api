# Provenance — SDK37 Strava

Baseline: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` (verified with
`git rev-parse HEAD`). Research and implementation date: **2026-11-07**.
Live provider calls: **NOT_RUN**. Only the three authorized public documentation
pages were fetched; no Strava API, OAuth service or real credential was used.
Every executable workflow uses `TestServer::start(|request: &Recorded| -> Reply)`.

This is **example-assisted through documentation excerpts**. No existing
`examples/*-sdk` source file was opened directly. The excerpt ledger below records
those examples too, rather than implying an unaided implementation.

## Official sources

| Exact page | Read date | Facts applied |
|---|---|---|
| https://developers.strava.com/docs/authentication/ | 2026-11-07 | OAuth code exchange, form-body curl examples, Bearer access, expiring tokens, refresh-token rotation and granted permissions |
| https://developers.strava.com/docs/rate-limits/ | 2026-11-07 | Overall plus non-upload header pairs, two calendar windows, 429, UTC resets and continued daily charging after short-window rejection |
| https://developers.strava.com/docs/uploads/ | 2026-11-07 | Multipart upload fields and file encodings, asynchronous receipt/status fields, 64-bit IDs, polling interval and processing/duplicate errors |

The authentication page's prose/parameter table and curl examples differ on token
URL prefix and parameter placement. This SDK uses `/oauth/token` from its explicit
refresh instructions and URL-encoded form bodies from its curl examples; it does
not put secrets in query strings. This choice is fixture-tested, not live-verified.

The permitted pages do not include a complete activity-endpoint reference. Activity
routes, numbered `page` / `per_page` conventions, filters and the small activity
projection are SDK contract choices for the requested slice; fixture data is
synthetic. The 1–200 page-size adapter bound and empty-page termination are
conservative SDK choices. No unconsulted endpoint-reference page is claimed as a
source. Local wire tests prove this implementation's behavior, not live API
conformance or file acceptance by Strava.

## Fixture derivation ledger

All values, IDs, names, secret canaries and status prose below are **authored
synthetic**, not copied provider responses. Keys follow the cited contract where
that contract is available. Expectations were authored from that contract and
these fixtures before running the client, not captured from SDK output.

| Fixture | Derivation and purpose |
|---|---|
| `activities-1.json` | Synthetic requested activity-list contract; full page and ID above JavaScript's exact integer range |
| `activities-2.json` | Synthetic short page with an unknown future sport; traversal must continue |
| `activities-empty.json` | Synthetic empty page; explicit traversal completion |
| `activity.json` | Synthetic activity-detail projection; additional unknown fields demonstrate partial models |
| `tokens.json` | Authentication page token fields; synthetic positive lifetime, accepted scopes and secret canaries |
| `expired-tokens.json` | Authentication token fields; synthetic expired initial state to trigger refresh |
| `rotated-tokens.json` | Authentication refresh fields; synthetic second rotation, with omitted optional scope |
| `upload-receipt.json` | Uploads page submission schema; synthetic queued receipt with 64-bit ID |
| `upload-pending.json` | Uploads page status schema; synthetic intermediate processing state |
| `upload-ready.json` | Uploads page completion schema; synthetic activity ID distinct from upload ID |
| `upload-failed.json` | Uploads page processing-error schema; synthetic duplicate/malformed error containing an HTML/secret canary |
| `problem.json` | Synthetic common HTTP error envelope; not asserted to be a verbatim Strava sample; used for the HTTP status matrix |
| `rate-windows.json` | Rate-limits page header semantics; synthetic short-only, daily-only, all-window and non-upload-only exhaustion cases |
| `track.gpx` | Synthetic GPX track with timestamped points, following the uploads page's time-per-point requirement; not externally schema-validated |
| `invalid.json` | Synthetic unfinished JSON with a canary for decode/redaction tests |

`protocol.rs` also authors synthetic bodies in the responder: wrong/missing media,
a body larger than 1 MiB, a truncated chunked body, failed upload byte source,
HTTP status matrix (302/400/401/403/404/409/422/429/500/503), quota parser edge cases
and unavailable response heads. `workflows.rs` authors mismatched IDs, a zero
lifetime, delayed receipts, failed refreshes and stateful 401/rotation sequences.
These generators live in tests rather than static files so transport truncation
and timeout behavior remain observable. None is a provider capture.

## Files and API documentation read

Required project guides: root `README.md`, `docs/QUICKSTART.md`,
`docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`.
Capability decisions followed reads of every requested README:
`crates/concord-{auth,pagination,retry,ratelimit,codecs}/README.md`.
Workspace and facade manifests and `Cargo.lock` were inspected for dependencies.

Rustdoc was read in source, including crate-level docs and the used public items:

- `crates/concord/src/lib.rs` and `crates/concord-macros/src/lib.rs`: declaration and generated API.
- `crates/concord-core/src/{lib,prepared,response,error,codec,field}.rs`: `Endpoint`, `SendWith`, request/response lifecycles, `Decoder::execute`, bounded documents, metadata, delivery and body encoding.
- `crates/concord-codecs/src/{lib,json,form,encoded,multipart}.rs` and `src/stream/upload.rs`: strict formats, runtime media, buffered multipart and lazy-body constraints.
- `crates/concord-auth/src/{lib,cache}.rs`: token, snapshot, generation, single-flight cache and invalidation.
- `crates/concord-pagination/src/{lib,pages,types}.rs`: page facade, prefix errors, metadata and continuation.
- `crates/concord-retry/src/lib.rs`: replay permission, scheduling, attempts and Tokio driver.
- `crates/concord-ratelimit/src/lib.rs`: atomic multi-key admission and monotonic observations.
- Cargo-registry `reqwest` 0.13.4 and locked 0.13.5 `src/async_impl/multipart.rs`: public form-stream, boundary and streamed-part APIs; 0.13.4 was inspected first, then the actual locked version was checked.

No parent campaign/role/worktree files were read. Directory inventories were used
to locate the authorized guides and crates; listing a filename is not a claim of
consulting its contents.

## Example assistance ledger

| File identified by the guide (excerpt only unless stated) | What was taken |
|---|---|
| `examples/quickstart/src/lib.rs` | Client/scope/endpoint syntax, `with_base_url`, required config, generated send and bounded provider documents |
| `examples/quickstart/src/main.rs` | Local fixture origin pattern; no server implementation copied |
| `examples/shapes/tests/wire.rs` | Read the DNS override excerpt; not used (Strava has no regional subdomain) |
| `examples/fred-sdk/src/lib.rs` | Explicit redirect, referer and native retry controls shown in AUTHOR_GUIDE |
| `examples/openai-sdk/src/lib.rs` | Buffered multipart declaration from guide; rejected for a one-shot streaming source |
| `examples/google-drive-sdk/src/upload.rs` | Runtime multipart media example; related multipart not used for Strava |
| `examples/frankfurter-sdk/src/lib.rs` | Send-policy pattern retaining status plus independently fallible bounded error document |
| `examples/native/src/native_edit.rs` | `into_parts` / native boundary pattern from RECIPES; interleaved-query recipe not used |
| `examples/compat/src/lib.rs` | Read the resource-segment validation excerpt; not used |
| `examples/testserver/src/lib.rs` (**opened directly**) | `Recorded`, `Reply`, delay/truncation/body-source controls and owned shutdown; used as dev dependency without edits |
| `examples/testserver/Cargo.toml` (**opened directly**) | Dev dependency package name and runtime features |

Other example links in the required guides were not followed. The compiler-facing
snippets in those guides informed the implementation; no other SDK implementation
was copied or used as an oracle.

## Dependencies and ownership

No new third-party crate or package-version change was introduced. Cargo added
only the new local `concord-sdk-strava` 0.1.0 package entry (license:
MIT OR Apache-2.0). Existing Concord crates remain version 4.2.0. Reused locked
third-party crates: reqwest 0.13.5 (streaming multipart), serde 1.0.229 (models and
forms), tokio 1.54.1 (mutex/timers), futures-util 0.3.34 (test byte sources) and
serde_json 1.0.154 (fixture parsing in tests). JSON and form serializers used by
Concord were already locked. No new-crate license justification is needed.

The first build was `CARGO_BUILD_JOBS=4 cargo test --offline -p
concord-sdk-strava --no-run`, without `--locked`, using the default `target/`.
Every later build/test/lint/doc command uses `--locked`. Cargo commands run one at
a time. Changes are limited to this package, the root member addition and Cargo's
new lockfile package entry. Shared crates, checks, scripts and other examples
were not edited. No commit was attempted.
