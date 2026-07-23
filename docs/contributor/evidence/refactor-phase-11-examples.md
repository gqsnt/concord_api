# Phase 11 examples-as-product-narrative evidence

Phase 11 was implemented in the worktree based on commit `7b46fd4`.
Validation used Rust/Cargo 1.97.1 on Windows, with the declared Rust 1.85
minimum checked separately.

## Importable product narrative

Every canonical `api!` declaration now lives below
`concord_examples::src` in a feature-gated capability module. The package
exports:

- `getting_started` for the minimal contract;
- `contracts` for the full DSL, regional scopes, and reusable explicit
  endpoints;
- `authentication` for the declaration, downstream provider/material
  extensions, and named configuration;
- `request_response` for bodies, generic specifications, and records;
- `pagination` for offset, page-number, cursor, and downstream-controller
  usage;
- `rate_limiting` for lexical plans, a downstream limiter, and an observer;
- `runtime_endpoints` for generated/runtime endpoint parity;
- `dangerous` for the two explicit dangerous capabilities;
- `riot` for the product-scale Riot and Data Dragon narrative.

The former shared extension dumping ground was removed. Custom codecs live
with specifications, credential material/provider types live with
authentication, page/controller types live with pagination, and limiter and
observer types live with rate limiting. Every implementation imports the
documented `concord` façade contracts.

Cargo metadata reports no example targets. The only binaries are:

| Binary | Required features | Responsibility |
|---|---|---|
| `minimal` | none | call the importable minimal constructor |
| `records` | `csv`, `ndjson` | construct the importable record requests and point to local proofs |
| `riot` | `riot-live` | call the imported environment-driven live workflow |

No binary declares an API. `riot-live` is the only target that reads live
service environment variables, and it was not run during validation.

Transport compression flags intentionally have no separate declaration
example because they do not change the generated Rust surface.
`dangerous-dev-tools` is internal test infrastructure and is explicitly
omitted from the application narrative.

## Riot split

The former single 1,360-line Riot binary is now divided into:

- domain routing/account/league/match/status/tournament/Data Dragon modules;
- independent `RiotApi` and `DDragonApi` declaration modules;
- a usage module containing runtime limiter composition and the manual-live
  workflow;
- the thin `riot` binary.

The complete declarations still compile. Deterministic tests use their exact
generated endpoint values and verify:

- `na1.api.riotgames.com` platform and
  `americas.api.riotgames.com` regional host composition;
- authentication capability with the API-key sentinel absent from safe URLs,
  sanitized headers, plans, and descriptions;
- two lexically composed rate-limit rules for representative endpoints (one
  scope block plus one endpoint block, with windows retained inside each
  rule);
- optional query absence and default pagination-query presence;
- public enum setters plus protected pagination bindings in safe planning;
- pagination terminal availability;
- explicit endpoint description/plan identity and capabilities;
- independent Data Dragon locale/path planning.

Safe planning deliberately omits privately applied authentication headers and
redacts pagination-bound query values; the example tests assert that contract
rather than depending on a native request preview.

## Deterministic execution

The synthetic `DeterministicApi` test was deleted. The replacement test
ownership is:

| Test | What it proves |
|---|---|
| `construction.rs` | imports every enabled canonical declaration and constructs representative direct, explicit, paginated, configured, and runtime requests |
| `local_runtime.rs` | executes the actual body, specification, authentication, pagination, rate-limit, records, and bounded raw-response examples against loopback support |
| `riot_contract.rs` | compiles and plans the actual Riot/Data Dragon declarations |

The all-feature examples suite contains one construction test, seven local
runtime tests, and three Riot contract tests.

CSV and NDJSON each execute twice through the actual `RecordsApi`: once with
one-byte scripted chunks and once with irregular chunks crossing headers,
records, CRLF, and final-record boundaries. The generic record decoder is
also constructed through the same declaration.

The authentication example keeps a fixed HTTPS contract. A bounded loopback
CONNECT tunnel was added to `concord-test-support` so the actual declaration
can execute against the existing trusted TLS fixture. The test stores an
endpoint response as credential material, then observes its application as a
query credential at the TLS origin. The tunnel:

- forwards only to an explicitly supplied loopback socket, never the
  requested public destination;
- bounds the CONNECT head to 16 KiB;
- uses finite connect, read/write, and idle bounds;
- joins its worker on drop.

All other runtime examples use scripted HTTP loopback proxies. No runtime test
calls a public service. The environment did not expose a repository command
or sandbox switch that enforces process-wide egress denial, so network-disabled
execution is not claimed; the live Riot binary was not run.

## Documentation and feature behavior

Every capability and Riot child module now states:

- what it teaches;
- required features;
- construction/local/manual-live mode;
- the owning guide/reference path;
- the proving acceptance test.

The default, all-feature, and no-default examples suites pass. In the
no-default suite, fixed-HTTPS declarations are still compiled while
construction is limited to their inert builders because core correctly
reports `TlsUnavailable` if such a client is built without the TLS feature.

Cargo metadata and the normal depth-one tree show that `concord-examples` has
only `concord` among normal Concord dependencies. `concord-test-support` is a
dev-only dependency with only its loopback feature. No canonical import names
core, extras, or macros.

## Security and lifecycle review

Phase 11 changed authentication examples, safe planning assertions, and
header/query behavior, so the full 17-test core seeded
redaction/status-body matrix was rerun and passed. The five façade
description/planning/lifecycle tests also passed.

The example-specific seeded proofs additionally cover:

- a Riot API-key sentinel absent from all safe plan/description surfaces;
- endpoint-backed credential acquisition over trusted loopback TLS;
- private credential placement visible only at the TLS origin;
- no external request before an example request is awaited;
- explicit downstream limiter precedence;
- generated and runtime endpoint execution through the same public request
  contract;
- bounded raw-success acquisition.

No native Reqwest request or response was exposed to an example extension.
No alternate executor was introduced.

## Commands and observed results

| Command | Result |
|---|---|
| Phase-start `cargo check -p concord-examples --all-targets` | PASS on the clean Phase 10 baseline |
| Phase-start `cargo check -p concord-examples --all-targets --all-features` | PASS on the clean Phase 10 baseline |
| `cargo fmt --all -- --check` | PASS |
| `cargo check -p concord-examples --all-targets` | PASS |
| `cargo check -p concord-examples --all-targets --all-features` | PASS |
| `cargo test -p concord-examples` | PASS; 1 construction, 5 local-runtime, and 3 Riot tests |
| `cargo test -p concord-examples --all-features` | PASS; 1 construction, 7 local-runtime, and 3 Riot tests |
| `cargo test -p concord-examples --no-default-features` | PASS; 1 construction and 1 locally executed generic-specification test |
| `cargo test -p concord-examples --doc --all-features` | PASS; zero doctest blocks |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p concord-examples --all-features --no-deps` | PASS; localized MSVC proc-macro linker warning remained |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo test -p concord-test-support --all-features` | PASS; 3 tests plus doctests |
| `cargo test -p concord --test endpoint_planning --all-features --quiet` | PASS; 5 tests |
| `cargo test -p concord-core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test proxy --test sanitized_metadata --test status_body_contract --quiet` | PASS; 17 tests |
| `cargo +1.85.0 check -p concord-examples --all-targets --all-features` | PASS |
| `cargo tree -p concord-examples --all-features -e normal --depth 1` | PASS; only `concord` among normal Concord dependencies |
| `cargo metadata --format-version 1 --no-deps` target/dependency inspection | PASS; one library, three thin binaries, three required acceptance tests, no Cargo example targets |
| `cargo clippy -p concord-examples --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to installed Rust 1.97.1 |
| `git diff --check` | PASS |

During implementation, the first no-default test correctly exposed that a
fixed-HTTPS client cannot be built without TLS. The construction proof was
changed to compile inert builders in that feature state, and the full
no-default rerun passed. The first CONNECT-tunnel attempt inherited a
nonblocking accepted socket on Windows, then a two-thread copy variant did
not terminate promptly with a pooled connection. The final bounded
single-worker duplex loop, explicit client drop, and executor yield passed
repeatedly. These were test-infrastructure corrections, not product-design
deviations.

## Deviations and remaining blockers

No selected design decision or product invariant required a deviation.

Clippy remains unavailable for the installed toolchain. Process-wide egress
enforcement was unavailable, so only loopback targeting—not firewall-level
network denial—is claimed. The manual rust-analyzer interaction remains a
release gate.
