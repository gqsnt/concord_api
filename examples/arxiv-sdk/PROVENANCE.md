# Provenance and observable actions

Authored 2026-11-07 against frozen baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`.
Live provider calls: **NOT_RUN**. No query route, article page or PDF was requested.
Only the official help page below was retrieved over the web. All execution used
`TestServer::start(|request: &Recorded| -> Reply)` on loopback.

## Provider documentation

| Exact page | Read date | Contract used |
| --- | --- | --- |
| https://info.arxiv.org/help/api/user-manual.html | 2026-11-07 | §§3.1.1.1–3.1.1.2: ID lists, query terms and offsets; §3.3: Atom, namespaces, optional extensions and metadata; §3.4: error feeds; §4.1: response shape; §5.1: field prefixes and phrase syntax. |

The web reader exposed the manual's text. No linked live example URL was opened.
HTTPS transport, strict Atom media acceptance, 100-entry pages, 50-ID input cap,
512-byte terms, 2 MiB/64 KiB body limits and 50,000 XML nodes are explicit SDK choices,
not claims of successful live interoperability. The API remains public and unauthenticated.

## Fixture derivations

Every fixture is synthetic and labelled in an XML comment. Names, titles, abstracts,
dates, DOI, counts and error wording are authored test data, not factual metadata.
The legacy IDs are syntax examples from the manual; the modern ID is invented.
The XML was authored from the documented field shapes before the workflow assertions;
no fixture or expected value was captured from SDK output.

| File under `fixtures/` | Derivation and purpose |
| --- | --- |
| `search-0.xml` | Synthetic §3.3/§4.1 feed; invented complete paper, repeated authors/affiliations, known extensions, entity text, unknown extensions and namespace collisions. |
| `search-1.xml` | Synthetic second offset page; alternative Atom/OpenSearch prefixes, CDATA, absent optional metadata and foreign DOI. |
| `ids.xml` | Synthetic ID-list feed combining the two authored records above with requested page size 50. Composed with Python's XML library, without executing the SDK. |
| `empty.xml` | Synthetic zero-result feed, retaining valid OpenSearch metadata. |
| `error-start-type.xml` | Synthetic §3.4 noninteger-start error class. |
| `error-start-negative.xml` | Synthetic §3.4 negative-start error class. |
| `error-max-type.xml` | Synthetic §3.4 noninteger-max_results error class. |
| `error-max-negative.xml` | Synthetic §3.4 negative-max_results error class. |
| `error-id.xml` | Synthetic §3.4 malformed-modern-ID error class; URI fragment and summary authored. |
| `error-legacy-id.xml` | Synthetic §3.4 malformed-legacy-ID error class with Unicode dash. |
| `error-max-large.xml` | Synthetic §3.1.1.2 over-30,000 error class. |
| `malformed.xml` | Synthetic mismatched closing tag. |
| `truncated.xml` | Synthetic XML ending mid-entry despite clean HTTP EOF. |

`protocol.rs` also creates explicitly synthetic adversarial variants: missing/duplicate
required fields, absent/signed/overflowing counts, wrong namespaces, DTD text, invalid
UTF-8, empty/non-Atom input, excess nodes, bad paging metadata, sentinel secrets,
oversized whitespace and transport interruption. Media labels, status codes, cookies,
redirect locations and chunk delivery are scripted test inputs. Exact wire assertions
come from the manual's parameter names and form-URL encoding rules; they are not
snapshots of `prepare()` output. Virtual-clock waits are checked independently, and a
real-clock check verifies the default delay through a client clone.

## Local files read

Required guides read: root `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md`. Also read root `Cargo.toml`, `Cargo.lock`,
and the READMEs of `concord-pagination`, `concord-retry`, `concord-auth`,
`concord-ratelimit`, and `concord-codecs` before deciding which capabilities fit.
The file inventory found no worktree `AGENTS.md`.

Rustdoc was read in its source form: `crates/concord/src/lib.rs`;
`crates/concord-core/src/{lib,codec,prepared,response}.rs`;
`crates/concord-codecs/src/{lib,media}.rs`;
`crates/concord-pagination/src/{lib,pages,offset}.rs`;
`crates/concord-ratelimit/src/lib.rs` and its `Cargo.toml`.
These supplied the `Decode`, `SendWith`, `Endpoint`, response-document, page-factory,
offset and clock/admission contracts. No shared source was edited.

Cargo-registry files read: `roxmltree-0.21.2/src/{lib,parse}.rs` and `Cargo.toml`
for namespaces, DTD defaults, node limits and license; crate-level rustdoc in
`reqwest-0.13.5/src/lib.rs` and `tokio-1.54.1/src/lib.rs`. The registry file inventory
also listed `reqwest-0.13.4`; its contents were not read. No files in parent campaign
directories or other worktrees were read.

## Examples consulted — example-assisted label

| Actual file opened | What was taken |
| --- | --- |
| `examples/testserver/Cargo.toml` | Exact harness package name/path and Tokio test setup. |
| `examples/testserver/src/lib.rs` | `Recorded` request assertions, `Reply::text`/headers, `Body::{Stall,FailAfter,Chunks}`, server start/requests/shutdown lifecycle. |

No existing file under `examples/*-sdk/` was opened. Required guides contain
embedded examples: the generic declaration/setter/`send` pattern and custom-format
and send-policy recipes informed this implementation. In particular, the
`examples/quickstart/src/lib.rs` excerpt in the quickstart and the
`examples/frankfurter-sdk/src/lib.rs` excerpt in AUTHOR_GUIDE §5.11 illustrate
bounded provider documents; these origin files were **not** opened. This is
documentation-assisted plus required-harness-example-assisted, with no claim of
example-independent implementation.

## Dependencies and lockfile

No registry crate was added and no existing package version changed. Cargo added
only the `concord-sdk-arxiv` package record. `roxmltree = 0.21.2` was already locked;
license **MIT OR Apache-2.0**. It supplies namespace-aware XML parsing with DTD and
node controls, avoiding a handwritten XML tokenizer. Concord, codecs, pagination,
ratelimit, reqwest, Tokio and the test harness were already in this workspace/lock.
The new SDK itself uses the workspace's MIT OR Apache-2.0 license.

The initial `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-arxiv --no-run`
completed successfully without `--locked` as required. Subsequent build/check/doc
commands use `--locked`. The first integration test run passed 20 tests and reported
missing crate-level test documentation; two module doc comments resolved those
warnings. Clippy then passed with `-D warnings`. Final results and line counts are
recorded in `VALIDATION.md`. All build output uses this worktree's default `target/`.
