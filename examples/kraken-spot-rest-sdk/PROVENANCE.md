# Provenance and observable work

Session retrieval date: **2026-11-07**. Baseline verified with `git rev-parse HEAD`:
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`. Initial `git status --short` was empty.
No `AGENTS.md` was found inside the worktree. No campaign/parent/worktree files,
other roles' material, real credentials or provider API responses were read.
**Live provider calls: NOT_RUN.** Documentation retrieval was read-only.

Official research pages

| Requested exact page, read 2026-11-07 | Resolved page | Used for |
|---|---|---|
| https://docs.kraken.com/exchange/guides/rest/authentication | Same URL | Header names, signature formula and fixed vector, unsigned increasing nonce, arrival-order caveat, OTP |
| https://docs.kraken.com/api/docs/rest-api/get-account-balance/ | https://docs.kraken.com/api-reference/account-data/get-account-balance | Balance route, query permission, result strings and suffixes, wallet query, rebase option |
| https://docs.kraken.com/api/docs/rest-api/get-open-orders/ | https://docs.kraken.com/api-reference/account-data/get-open-orders | Open-orders route, query permission, option names, order sample, default complete read without cursor/limit |

The endpoint pages label request bodies JSON; the authentication page also shows
URL-encoded payload signing. This SDK deliberately selects the form variant.
No additional official pages were fetched. The public route/schema cases are
synthetic and are not represented as newly researched public API documentation.

Fixture derivation

Every fixture is checked in. Expected signatures were computed outside the SDK
with Python's standard library, not by calling the Rust signer. The official
vector remains a separate external known answer. The AddOrder path in that pure
cryptographic fixture does not introduce an order operation or send any request.

| File | Derivation / expected facts |
|---|---|
| `fixtures/official-signature.json` | Authentication page's fixed key, nonce, exact encoded body, path and signature; added explanatory origin label |
| `fixtures/wire-vectors.json` | Synthetic key/secret, nonce 9001 balance OTP containing plus/space/ampersand/equal/slash/Unicode and nonce 9002 filtered orders; independent Python URL encoding and HMAC expected bytes |
| `fixtures/account-signatures.json` | Synthetic default-option balance nonce 5001 and orders nonce 5002; independent Python signature expectations |
| `fixtures/balances.json` | Selected five unchanged entries from the account balance page's response example, including suffixed assets |
| `fixtures/open-orders.json` | First order from the open-orders page's response example; second order omitted; retained prices, quantities, times and trade ID |
| `fixtures/pairs.json` | Authored synthetic two-pair market fixture, one online and one under maintenance; no real market snapshot claim |
| `fixtures/ticker.json` | Authored synthetic ticker with fixed decimal strings and known positional arrays; no live price claim |
| `fixtures/api-errors.json` | Authored synthetic error messages and expected SDK classes; InvalidNonce spelling and failure semantics appear in the auth guide; other class cases are synthetic compatibility examples, not a claim that the allowed pages enumerate them |
| `fixtures/malformed.json` | Authored synthetic incomplete JSON document, delivered with a clean HTTP end |
| `fixtures/missing-result.json` | Authored synthetic empty-error envelope with no result |
| `fixtures/wrong-shape.json` | Authored synthetic array where a balance map is expected |
| `fixtures/signatures.py` | Independent offline oracle/reproduction script for the three signature fixture files; never imports or reads SDK implementation |

Inline fixture variants are synthetic: protocol tests wrap error-list strings,
use statuses 307/400/401/403/404/429/500/503, add fake sensitive echoes and malformed
or repeated media/Retry-After fields, send blank/invalid-UTF-8 bodies, generate
`limit + 1` bytes, cut chunked responses short, close before a head, return null
results, and remove optional order fields. Workflow variants duplicate a pair
alias, return absent/mismatched tickers, refuse a permission at either account
step, track concurrent nonces, and implement a strict increasing-arrival fixture.
They do not reproduce implementation-generated expected values. The controlled
arrival fixture explicitly rejects a delayed lower nonce despite both allocations
being distinct. No test asserts FIFO arrival for ordinary concurrent sends.

Local reading and assistance ledger

This SDK is **example-assisted**. No existing `examples/*-sdk` file was directly
opened. Direct example file consultation was limited to:

| Example file | What was taken |
|---|---|
| `examples/testserver/Cargo.toml` | Package name/path and required runtime features for the dev dependency |
| `examples/testserver/src/lib.rs` | Rustdoc and API for `TestServer::start`, `Recorded` path/header/form inspection, `Reply`, `Body::FailAfter`, `Gate`, request accounting and shutdown |

The required guides themselves contain example excerpts. The relevant assistance
was: `README.md` and `docs/QUICKSTART.md`'s quickstart declaration/construction and
local-server pattern; `docs/AUTHOR_GUIDE.md` §3's FRED transport excerpt, §5.11's
Frankfurter bounded error-policy excerpt and §5.13's policy/signing examples;
`docs/RECIPES.md`'s native request-edit, response-metadata and policy recipes.
These are documentation excerpts, not direct reads of their originating files.
The multipart, streaming, application-traversal and other-provider excerpts in
the guides were read as guide content and supplied no implementation here.
`FRICTION.md` lists the exact originating examples relevant to adoption.

Required repository pages read: `README.md`, `docs/QUICKSTART.md`,
`docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`. Capability READMEs
read before selection: `crates/concord-{core,macros,auth,pagination,retry,ratelimit,codecs}/README.md`.

Rustdoc/source reads for used public APIs: `crates/concord/src/lib.rs`,
`crates/concord-core/src/{lib,prepared,codec,response,error}.rs`,
`crates/concord-codecs/src/{lib,form,json}.rs`,
`crates/concord-retry/src/{lib,http}.rs`. Build metadata read:
root `Cargo.toml`, `Cargo.lock`, `crates/concord-retry/Cargo.toml` and the
testserver manifest. File inventories and symbol searches were restricted to the
worktree and the cargo registry. No other SDK content was inspected.

Cargo registry reads: `hmac-0.13.0/src/lib.rs`, `hmac-0.13.0/README.md`,
`sha2-0.11.1/src/lib.rs`, `sha2-0.11.1/README.md`, and
`base64-0.23.1/src/lib.rs` for `KeyInit`, `Mac`, `Digest` and standard Base64
interfaces, and the hmac/sha2/base64 manifests for licenses. The compiler's own
warning supplied the atomic API rename. No external general web research was used.

Dependencies and lockfile

No new registry package is introduced. All package versions already occur in the
frozen lockfile; the only added lock entry is `concord-sdk-kraken-spot-rest 0.0.0`.
The SDK uses path dependencies on Concord and concord-retry, serde for models,
and the following already-locked cryptographic dependencies directly:

| Crate/version | License | Why a direct dependency is necessary |
|---|---|---|
| `base64 0.23.1` | MIT OR Apache-2.0 | Decode provider secret and encode HMAC output |
| `hmac 0.13.0` | MIT OR Apache-2.0 | HMAC-SHA512 primitive; avoids hand-written cryptography |
| `sha2 0.11.1` | MIT OR Apache-2.0 | SHA256 payload hash and SHA512 HMAC algorithm |

`serde_json` is an already-locked dev dependency for fixture variants and oracle
records. Tokio and the existing testserver are dev dependencies. The package
turns on Concord `json` and `form`, with default features off. It does not add TLS
or another HTTP implementation. No dependency version, source or checksum was
changed for any existing package.

Observable decisions and diagnostics

1. Verified the baseline and clean tree; inventoried local instructions/files.
2. Read required guides, capability READMEs and public API rustdoc in source;
   retrieved only the three authorized documentation pages.
3. Added one member, typed declaration, models, error policy and exact-byte signer.
4. First build: `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-kraken-spot-rest --no-run`.
   Succeeded and added only the SDK lock entry. One Rust 1.99.1 deprecation warning
   identified `AtomicU64::fetch_update`; changed it to `try_update`.
5. Independent Python fixtures and protocol tests passed; added both connected
   workflows, concurrent cloned sends and deterministic reversed-arrival failure.
6. Subsequent cargo commands use `--locked`, one command at a time, four build
   jobs, default worktree `target/`. Final gate outcomes are recorded below.

Validation record

All commands below passed on 2026-11-07, sequentially. No `CARGO_TARGET_DIR` was
set; generated build and API documentation files are in the worktree's `target/`.

| Command | Result |
|---|---|
| `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-kraken-spot-rest --no-run` | First build succeeded without `--locked`; only the SDK entry was added |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-kraken-spot-rest --test protocol` | Initial 14 protocol tests passed; final suite expanded to 16 |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-kraken-spot-rest` | Final: 16 protocol + 7 workflow tests + 1 executable README doctest passed |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-kraken-spot-rest --all-targets -- -D warnings` | Passed, no warnings |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-kraken-spot-rest --no-deps` | Passed, API docs generated |
| `python3 examples/kraken-spot-rest-sdk/fixtures/signatures.py` | Five independent signature vectors matched |
| `rustfmt --edition 2024 examples/kraken-spot-rest-sdk/src/*.rs examples/kraken-spot-rest-sdk/tests/*.rs` | Owned Rust files formatted |
| `git diff --check` | Passed |

A Python TOML comparison against `git show HEAD:Cargo.lock` verified equality of
every existing package entry (including versions, sources, checksums and dependency
lists); the only added entry is this package. Final scope audit: root Cargo.toml
has only the new workspace member, Cargo.lock has only its package entry, and all
other authored files are under `examples/kraken-spot-rest-sdk/`.

Physical line counts, including comments and blank lines: source **785** in five
Rust files (`auth.rs`, `error.rs`, `lib.rs`, `models.rs`, `policy.rs`); tests **882**
in `protocol.rs` and `workflows.rs`; fixtures **263** in 11 JSON files (201 lines)
and the Python oracle (62 lines). The three Markdown deliverables and the package
manifest are additional files, excluded from those categories.

No commit is created; changes remain in the working tree. No shared crate, check,
script or other example is edited. Live provider calls remain **NOT_RUN**.

Maintenance: trade history (2026-11-07)

Independent maintainer, after the first consumer. Edits are confined to this
package; `Cargo.lock` gained one dependency line in this package's own entry.
**Live provider calls: NOT_RUN.** `MAINTENANCE.md` has the change record.

| Requested exact page, read 2026-11-07 | Resolved page (HTTP redirect observed) | Used for |
|---|---|---|
| https://docs.kraken.com/api/docs/rest-api/get-trade-history | https://docs.kraken.com/api-reference/account-data/get-trades-history | Route, permission, `account_id` query, every body option and default, `start` exclusive / `end` inclusive (timestamp or trade id), `ofs` (marked deprecated in favour of `with_cursor`/`cursor`), `limit` default 50 (1–100), `count` (0 with `without_count` or `with_cursor`), trade fields, response example |
| https://docs.kraken.com/api/docs/guides/spot-rest-auth | https://docs.kraken.com/exchange/guides/rest/authentication | Same page as the author's authentication source: signature over URI path and SHA256(nonce + POST data), increasing per-key nonce, arrival-order caveat, OTP |

Both pages were fetched read-only once with `curl -L` into
`target/tmp/maint/` (not committed) and read from extracted text and the
page's embedded OpenAPI JSON. The reference request examples are JSON bodies;
this SDK keeps the form encoding the authentication guide signs (as before).

| File | Derivation / expected facts |
|---|---|
| `fixtures/trades-history.json` | Verbatim response example of the trade-history reference (two trades of order `OQCLML-BW3P3-BUCMWZ`); it has **no** `count` |
| `fixtures/trades-signatures.json` | Synthetic key/secret (same as `wire-vectors.json`); a default first page (nonce 6001) and a three-page filtered walk (nonces 7001–7003, `ofs` absent/50/100) with OTP special characters, `type=no position`, timestamp `start`, trade-id `end`; independent Python URL encoding and HMAC, checked by `fixtures/signatures.py` |
| inline in `tests/trades.rs`, `tests/trades_consumer.rs` | Synthetic multi-page ledgers (shape of the reference example, generated ids `TS…`/`TC…`), failure pages, inconsistent pages (missing count, 51 trades, empty page before count, shrinking count), a strict-arrival provider and a throttle-once provider |

The vectors were generated once by a stdlib-only script kept in
`target/tmp/maint/gen/` (never reading SDK code); the checked-in oracle
re-verifies them. Field order in the walk vectors is the SDK's declared field
order: an SDK choice, signed as sent.

Local reading for this change: the package's own files (source, tests, fixtures,
README, PROVENANCE; FRICTION.md and FIRST_USE.md only after the change was
implemented and tested), `docs/{CHEATSHEET,RECIPES,CALLER_GUIDE}.md` pagination
sections, `docs/AUTHOR_GUIDE.md` §2, §2.1, §5, §5.6 and §9,
`crates/concord-pagination/{README.md,Cargo.toml,src/lib.rs,src/offset.rs}`, the
public parts of `src/pages.rs` and `src/types.rs`, `crates/concord-codecs/src/form.rs`,
`crates/concord-core/src/response.rs` (Operation) and `examples/testserver/src/lib.rs`.
No other example, no campaign file and no other worktree was opened.

Dependency added: `concord-pagination` (workspace path crate, version 4.2.0,
MIT OR Apache-2.0, already a locked workspace member). Why: the shared
dependent-page facade (`Pages`: opt-in cap, `Bounded`/`Provider` errors with a
resume point, `collect_complete` keeping the completed prefix) and the checked
offset continuation `next_offset`. Its only dependency, `futures-util`, was
already locked. No registry package, version, source or checksum changed.

Maintenance validation (2026-11-07, sequential, `CARGO_TARGET_DIR` = the
worktree's absolute `target`, `CARGO_BUILD_JOBS=4`):

| Command | Result |
|---|---|
| `cargo build --offline -p concord-sdk-kraken-spot-rest` (once, without `--locked`) | Built; lock diff is one dependency line in this package's entry |
| `cargo test --locked -p concord-sdk-kraken-spot-rest` | first_use 11, protocol 16, workflows 7, trades 11, trades_consumer 1, doctests 2: all passed |
| `cargo clippy --locked -p concord-sdk-kraken-spot-rest --all-targets -- -D warnings` | Passed after replacing `% 2 == 0` with `is_multiple_of` and naming one fn-pointer type in tests |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-kraken-spot-rest --no-deps` | Passed |
| `python3 -I examples/kraken-spot-rest-sdk/fixtures/signatures.py` | Nine independent signature vectors matched |
| `rustfmt --edition 2024 --check` on the package's `src/*.rs` and `tests/*.rs` | Clean; pre-existing test files were not reformatted |
| `git diff --check`; Python `tomllib` comparison of `Cargo.lock` with `HEAD` | Clean; only this package's entry changed (`+concord-pagination`) |
