# Provenance — SDK25 FRED

Baseline: `56a16ee34f8aa714bd51dca67f744e5870e813b6`.
Reading date: **2026-11-07 (Europe/Paris)**; tool timestamps during this work begin
2026-11-06 23:24 UTC. Live provider calls: **NOT_RUN**. Only documentation and
dependency sources were fetched; no request went to the FRED API service.

## Official provider documentation

| Exact page | Retrieved | Contract used |
|---|---|---|
| <https://fred.stlouisfed.org/docs/api/fred/series_search.html> | 2026-11-07 | Search route, query key, default XML, JSON alternative, `seriess`, count/offset/limit and search limit range |
| <https://fred.stlouisfed.org/docs/api/fred/series_observations.html> | 2026-11-07 | Observations route, string values, default XML/JSON, CSV transported in ZIP with text README, observation limit/offset |
| <https://fred.stlouisfed.org/docs/api/fred/series.html> | 2026-11-07 | Selected-series query and metadata fields, optional notes, XML/JSON envelopes |
| <https://fred.stlouisfed.org/docs/api/fred/errors.html> | 2026-11-07 | XML/JSON problem shape, HTTP 400/404/423/429/500, documented rate limit; JSON example is labelled text/xml |

No documentation sample data, titles, notes, error prose or API keys were copied
into fixtures. Synthetic corpus values and expected workflow outputs were authored
separately from Rust decoding. The documentation establishes the wire contracts;
fixtures establish deterministic tests, not current service behavior.

## Fixture derivation inventory

Every fixture below is **authored synthetic**, contract-shaped by the cited pages.
`fixtures/generate.py` imports no SDK code, makes no network requests and regenerates
the checked-in data with fixed ZIP timestamps. Expected IDs, strings and output
bytes in tests come from this corpus, not from serializing SDK models.

| Files under `fixtures/` | Derivation and purpose |
|---|---|
| `generate.py` | Synthetic corpus authoring recipe, Python standard library only |
| `search-0.xml`, `search-1.xml`, `search-0.json`, `search-1.json` | Search-page structure from series_search; two invented IDs, one per page, count 2; ampersand/entity handling and absent optional notes |
| `metadata.xml`, `metadata.json` | series metadata shape; invented `SYN_A` fields matching the selected search item |
| `observations-0.xml`, `observations-2.xml`, `observations-0.json`, `observations-2.json` | series_observations page structure; count 3, offsets 0/2, limit 2; invented `2.5000`, `.`, `NA` values |
| `observations.csv`, `README.txt` | Invented CSV layout, missing-value spellings and README; no claim that FRED uses this exact layout |
| `observations.zip` | DEFLATE ZIP with the above CSV and README, per documented container contract |
| `stored.zip` | Same CSV in a stored ZIP without README; supported SDK compatibility shape |
| `oversized-extracted.zip` | DEFLATE member of 4 MiB + 1 byte; synthetic decompression bound counterexample |
| `oversized-total.zip` | 3 MiB CSV plus 2 MiB README; synthetic aggregate-size counterexample |
| `too-many-entries.zip` | Nine synthetic members; entry-count bound |
| `ambiguous.zip` | Two different CSV members; no arbitrary first-file selection |
| `no-csv.zip` | README only; missing required member |
| `unsafe-path.zip` | `../SYN_A.csv`; archive paths never become output paths |
| `unexpected-member.zip` | CSV plus synthetic executable name; membership policy |
| `symlink.zip` | UNIX symlink metadata; refused before extraction |
| `truncated.zip`, `malformed.zip` | Prefix-only ZIP and invented invalid central directory |
| `bad-crc.zip` | Stored CSV payload changed without changing its CRC |
| `encrypted.zip` | Encryption flags set in local/central headers; unsupported password archive |
| `unsupported-method.zip` | Method 99 in local/central headers, with no AES support |
| `forged-size.zip` | Declared uncompressed length changed to one byte; metadata is not proof of successful extraction |
| `error.xml`, `error.json` | errors-page structure, synthetic secret-echo message and document code 400 |
| `malformed.xml`, `malformed.json`, `dtd.xml` | Synthetic incomplete documents and disabled DTD/entity syntax |

Runtime fixtures in tests are also authored synthetic: `oversized_archive()` writes
a valid stored ZIP over the wire cap; chunked/stalled/cut-off bodies use TestServer;
media/header/status variations, numeric JSON values, malformed model envelopes and
pagination contradictions are specified directly in `protocol.rs`/`workflows.rs`.
They are transformations of this corpus, not captures from the provider. The
HTTP-date fixture is an old fixed date, so no real-time provider clock is needed.

## Dependencies and licenses

Already locked dependencies are preferred: `roxmltree 0.21.2` (MIT OR Apache-2.0)
parses XML, `mime 0.3.17` checks head fields, serde/serde_json model data, Tokio runs
tests, and Concord owns request composition, JSON, send policy, pagination and
Retry-After parsing. `flate2 1.1.10` (MIT OR Apache-2.0) already existed in the lock;
its explicit dependency activates the Rust DEFLATE backend without zopfli or C
compression features.

| Newly added lock package | License | Why needed |
|---|---|---|
| `zip 2.4.2` | MIT | ZIP central/local metadata, stored/DEFLATE members, CRC checks and malformed/encrypted archive rejection; avoids a handwritten archive parser |
| `crossbeam-utils 0.8.23` | MIT OR Apache-2.0 | ZIP's conditional 32-bit ARM/MIPS/PowerPC dependency; locked for portable resolution, not built on this x86_64 run |
| `arbitrary 1.5.0` | MIT OR Apache-2.0 | ZIP's `cfg(fuzzing)` dependency, locked but not built here |
| `derive_arbitrary 1.5.0` | MIT OR Apache-2.0 | Transitive derive dependency of the above conditional dependency, not built here |

ZIP is pinned exactly and uses `default-features = false` with `deflate-flate2`
and `flate2`. Feature setup was corrected after its initial build failure; see
EVIDENCE. No pre-existing lock package changed version or disappeared (verified
by comparing parsed `(name, version)` sets to `git show HEAD:Cargo.lock`).

Dependency documentation read on 2026-11-07:

- <https://docs.rs/crate/zip/2.4.2/features> (available); the initial attempt at
  <https://docs.rs/zip/2.4.2/zip/read/struct.ZipArchive.html> failed, so the fetched
  registry source's `src/lib.rs` and `src/read.rs` rustdoc and Cargo.toml were read.
- <https://docs.rs/crate/crossbeam-utils/0.8.23> plus its cached Cargo.toml.
- <https://docs.rs/crate/arbitrary/1.5.0> and
  <https://docs.rs/crate/derive_arbitrary/1.5.0> were unavailable; licenses were
  verified from Cargo.toml in the exact official registry archives:
  <https://static.crates.io/crates/arbitrary/arbitrary-1.5.0.crate> and
  <https://static.crates.io/crates/derive_arbitrary/derive_arbitrary-1.5.0.crate>.
- Cached `roxmltree-0.21.2/src/parse.rs` rustdoc (`ParsingOptions`, DTD/node limits),
  its Cargo.toml, `flate2-1.1.10/Cargo.toml`, and reqwest 0.13.5
  `src/retry.rs` / `src/async_impl/client.rs` rustdoc for transport retry control.
- Cached crate-level rustdoc from `mime-0.3.17`, `flate2-1.1.10`,
  `serde_json-1.0.154`, `serde-1.0.229`, `tokio-1.54.1` and `bytes-1.12.1`
  `src/lib.rs` was also read for media parsing, backend selection, model decoding,
  test runtime and byte ownership.

## Workspace reading and assistance record

Read in this worktree: `README.md`, `docs/QUICKSTART.md`,
`docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`, root Cargo.toml
and relevant lock entries. No AGENTS.md or CLAUDE.md existed inside the worktree
or `examples/`; ancestor `AGENTS.md`/`CLAUDE.md`, `START_HERE.md`,
`DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`, `contracts/PHASES.md` and
`contracts/RESOURCES.md` were read. The user's bounded capsule supersedes the
broader coordinator mission and forbids shared edits/commits.

Crate rustdoc read from source: `crates/concord/src/lib.rs`,
`crates/concord-core/src/{lib,codec,prepared,response,error}.rs`,
`crates/concord-codecs/src/{lib,json}.rs`,
`crates/concord-pagination/src/{lib,pages,types}.rs`,
`crates/concord-retry/src/{lib,http}.rs`,
`crates/concord-macros/src/lib.rs`. Manifests were inspected for Concord and
retry features. A focused symbol search in `concord-macros/src/emit.rs` checked
generated Debug locations after a test assumption failed; no macro code was
changed.

Example source files consulted directly (exhaustive):

| File | What was taken |
|---|---|
| `examples/testserver/Cargo.toml` | Package name and required test runtime support |
| `examples/testserver/src/lib.rs` | `TestServer::start`, Recorded query/header helpers, Reply/Body failure and stall scripting, explicit shutdown and request-count assertions |

No existing `examples/*-sdk` source file was opened. Required guides embed
example excerpts: Quickstart's Tasks declaration/caller showed ordinary send and
sensitive values; AUTHOR_GUIDE's `examples/frankfurter-sdk/src/lib.rs` excerpt
showed a generic send policy and typed bounded HTTP problem handling (adopted).
Its `examples/openai-sdk/src/lib.rs` and
`examples/google-drive-sdk/src/upload.rs` multipart excerpts were read as part of
the guide, but supplied no multipart implementation here. RECIPES' embedded
`examples/native/src/native_edit.rs` and `examples/compat/src/lib.rs` excerpts
were read but supplied no native-edit code. CHEATSHEET examples reinforced scope
inheritance and format selection. No private reasoning transcript or independent
reviewer/consumer claim is made; this is one author implementation.
