# Provenance

Read/retrieval date: **2026-11-07**. Baseline:
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` (verified with `git rev-parse HEAD`).
Live provider calls: **NOT_RUN**. No token acquisition, live account request or mutation.

## Official pages read

| Exact page | Read date | Facts used |
| --- | --- | --- |
| https://docs.digitalocean.com/products/droplets/reference/api/droplets/ | 2026-11-07 | Droplet list/get paths, bearer auth, selected infrastructure/network fields, page parameter ranges and response/error envelopes. |
| https://docs.digitalocean.com/reference/api/reference/public-apis/ | 2026-11-07 | Body next links and optional links, JSON error fields, numeric quota headers and burst delay. |

Only these two external pages were opened. No linked Actions page was read: it is
outside the explicitly permitted pages. Droplet-scoped action paths and selected action
fields are SDK author choices implementing the capsule's required workflow, exercised
with synthetic fixtures; they are not claimed to be copied or independently verified
from a retrieved Actions schema. This is a documentation coverage limitation.

No provider example payload or explanatory prose was copied wholesale. All fixture
data and messages were authored for this package. Documentation supplied field names,
types and envelope conventions; the capsule supplied the action-workflow requirement.

## Fixture derivation table

Every file below is **authored synthetic**, not a captured provider response. Reserved
documentation addresses are used for modeled networks. `__ORIGIN__` is replaced with
the active testserver origin; `__UNTRUSTED__` with a separate loopback trap's hostname.

| Fixture | Derivation and intentional differences | Tested contract |
| --- | --- | --- |
| `droplets-first.json` | Droplets page field/envelope conventions; synthetic IDs/names, selected fields, extra future field, two-page links. | List envelope, selected fields, unknown-field tolerance, continuation. |
| `droplets-last.json` | Same conventions; synthetic second droplet and prev-only pages. | Missing next completes even when other links exist. |
| `droplet.json` | Droplets single-object envelope; same synthetic selected resource. | List → get identity and nullable private gateway. |
| `droplets-no-links.json` | Public APIs' optional links contract, synthetic one-item page. | Omitted links means completion. |
| `empty-next.json` | Synthetic empty intermediate collection with body next. | Empty does not mean finished. |
| `unsafe-next.json` | Synthetic adversarial next URL based on public pagination shape. | Never contact an untrusted host, for either listing. |
| `actions-first.json` | Capsule-driven synthetic droplet action envelope/fields; same public pagination convention. | Selected droplet → paginated actions. |
| `actions-last.json` | Synthetic unfinished second action; null completion/region; omitted links. | Action nullability and completion. |
| `action.json` | Capsule-driven synthetic single-action envelope; stable identity from list. | Retrieve chosen droplet action. |
| `errors.json` | Synthetic status/document rows based on both pages' general error contract. | 400/401/403/404/429/500/503/default classes on every endpoint. |
| `malformed.json` | Intentionally invalid synthetic JSON. | JSON syntax failure. |
| `truncated.json` | Intentionally incomplete synthetic JSON. | EOF decode and interrupted chunked transport. |
| `wrong-shape.json` | Valid synthetic JSON with collection string rather than array. | Type mismatch without leaking text. |
| `error-secret.json` | Synthetic valid problem carrying the token marker in every text field. | Safe formatting and explicit unredacted field access. |

Runtime fixtures authored in tests (also **synthetic**): optional link omission/null
variants for both collection envelopes; wrong/absent/duplicate media labels; empty and
invalid UTF-8 bodies; spaces of exactly the selected byte limit plus one; incomplete
chunked delivery via `Body::FailAfter`; connection close before headers; redirect status
matrix with Location and Set-Cookie markers; invalid/repeated/overflowing quota fields;
same-origin non-collection, different-port/scheme, userinfo, fragment, relative, empty,
unparseable, control and backslash next URLs. The testserver responder matches literal
documented request targets, independently of the SDK declaration.

## Workspace reading and example assistance

**Classification: example-assisted through guide-embedded snippets and test support.**
No existing `examples/*-sdk` source file was opened directly. No SDK source was copied.
The complete directly opened example-file inventory is:

| File | What was taken |
| --- | --- |
| `examples/testserver/Cargo.toml` | Package name/version and path dependency/features for fixture support. |
| `examples/testserver/src/lib.rs` | Rustdoc and public API of Recorded, Reply, Body and TestServer: scripted responses, wire inspection, interrupted bodies, owned shutdown. No server implementation copied. |

Guide-embedded example excerpts were also read (files below were **not opened**):

| Excerpt source identified by guide | What was taken |
| --- | --- |
| `examples/quickstart/src/lib.rs` | Declaration hierarchy, sensitive bearer template, generated calls, bounded error-document pattern. Used the pattern, with shared auth HeaderValue instead of string formatting. |
| `examples/quickstart/src/main.rs` | Loopback runner concept; no binary copied. |
| `examples/shapes/tests/wire.rs` | Read the DNS override example; no subdomain/DNS override adopted. |
| `examples/fred-sdk/src/lib.rs` | Guide excerpt disabling redirect/referer/native retry; adopted this transport configuration idea for bearer destination control. |
| `examples/openai-sdk/src/lib.rs` | Read multipart excerpt while reading the full author guide; nothing taken for this JSON-only slice. |
| `examples/google-drive-sdk/src/upload.rs` | Read related-multipart excerpt; nothing taken. |
| `examples/frankfurter-sdk/src/lib.rs` | Guide excerpt showing status-authoritative typed bounded error documents; adopted policy pattern. |
| `examples/native/src/native_edit.rs` | Recipe-embedded interleaved query helper read; no native query editing adopted. |
| `examples/compat/src/lib.rs` | Recipe-embedded multisegment helper read; no native path editing adopted. |

Other example paths mentioned as links by the guides were not opened or consulted as
source. README/guide snippets are evidence of patterns, never provider expectations.

Required local documents read: `README.md`, `docs/QUICKSTART.md`,
`docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`, root `Cargo.toml`.
No AGENTS.md was found within the worktree by filename search. No parent campaign,
other role/worktree, mission, contract or record files were read.

Shared capability READMEs read before decisions: `crates/concord-pagination/README.md`,
`crates/concord-auth/README.md`, `crates/concord-retry/README.md`,
`crates/concord-ratelimit/README.md`, `crates/concord-codecs/README.md`.
Rustdoc read in source: `crates/concord/src/lib.rs`, `crates/concord-core/src/lib.rs`,
`prepared.rs`, `response.rs`, `error.rs`, `field.rs` in concord-core;
`crates/concord-pagination/src/lib.rs` and `pages.rs`;
`crates/concord-auth/src/lib.rs` and `credentials.rs`;
`crates/concord-codecs/src/lib.rs` and `json.rs`. This covers the public APIs used.
Supporting Cargo manifests for auth, pagination and testserver were also read.

Cargo registry rustdoc introductions read: reqwest 0.13.5 (`src/lib.rs`, client/redirect/
proxy sections), serde 1.0.229 (`src/lib.rs`, derive/trait introduction), serde_json
1.0.154 (`src/lib.rs`, Value/from_str), tokio 1.54.1 (`src/lib.rs`, runtime/test-feature
introduction). `rustc --version` reported 1.99.1. Only permitted worktree, toolchain and
Cargo registry paths were accessed locally.

## Dependency provenance

No new registry crate and no registry version update. Added only this package's lock
entry, using existing Concord 4.2.0 path crates, serde 1.0.229 (MIT OR Apache-2.0),
serde_json 1.0.154 (MIT OR Apache-2.0, tests only), tokio 1.54.1 (MIT, tests only), and
concord-example-testserver 0.0.0 (workspace license, tests only). Concord's existing
reqwest/default TLS stack is used via re-export. Existing registry license expressions
come from those locked crates' manifests; no new-crate justification is needed.

First build was `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-digitalocean`
without `--locked`, permitting Cargo to add its package entry. Subsequent Cargo checks
used `--locked --offline`. Default worktree `target/` was used; no CARGO_TARGET_DIR set.
No shared crate, other example, check or script was edited. No commit attempted.

## Validation record

All commands ran sequentially, with `CARGO_BUILD_JOBS=4`, in the default worktree
target directory on Rust 1.99.1. Results on 2026-11-07:

| Command (after the jobs environment setting) | Result |
| --- | --- |
| `cargo test --offline -p concord-sdk-digitalocean` | Added only the new package lock entry; initial compile rejected two empty path templates. Corrected by omitting those paths. |
| `cargo fmt -p concord-sdk-digitalocean` | Passed. |
| `cargo test --locked --offline -p concord-sdk-digitalocean` | Passed: 11 protocol tests, 6 connected-workflow/regression tests, 1 compiled README doctest. |
| `cargo clippy --locked --offline -p concord-sdk-digitalocean --all-targets -- -D warnings` | Passed. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-digitalocean --no-deps` | Passed. |
| `git diff --check` | Passed. |
| TOML comparison against `git show HEAD:Cargo.lock` and `HEAD:Cargo.toml` | Every preexisting lock package entry identical; workspace differs only by the authorized member. |

Source inventory: 4 Rust source files / 600 logical lines; 3 test/support files / 774
logical lines; 14 fixture files / 272 logical lines. A final unterminated fixture line
counts as one logical line. Fixtures also include runtime synthetic variants described
above. No live-service capability was exercised by these checks.
