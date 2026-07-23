# Phase 5 public namespaces, feature ownership, and one-crate consumption

Phase 5 was implemented from commit `b0ffa5d` with Rust and Cargo 1.97.1.
MSRV validation used Cargo 1.85.0.

## Resulting package and dependency contract

Cargo package identities now use conventional hyphens while Rust crate paths
remain underscores:

```text
concord
├── concord-core
├── concord-extras ──> concord-core
└── concord-macros

concord-examples ──> concord
concord-perf ──────> concord + concord-test-support
```

The façade has exactly three normal Concord-package edges. Core has no edge to
the façade, extras, or macros. Extras has only the intended core edge among
product packages. Macros has no normal core, extras, or façade edge. Every
workspace sibling edge uses `=0.1.0` and `default-features = false`; development
consumers name the support features they require rather than activating its
defaults implicitly.

`concord-core` and `concord-extras` now have empty defaults. `concord` owns the
ordinary default set:

```text
default-tls
http2
rate-limit-governor
```

JSON remains opt-in. CSV and NDJSON imply generic records support. Built-in
pagination controllers remain featureless. Transport, compression, multipart,
records, and dangerous features forward to core; codecs, OAuth2, and governor
forward to extras. A metadata policy test fixes the complete feature maps,
empty implementation defaults, exact sibling edges, docs.rs metadata, and the
reviewed pre-release publication posture.

## Public façade contract

The façade now provides the complete domain set:

```text
concord::auth
concord::body
concord::codec
concord::endpoint
concord::error
concord::hooks
concord::http
concord::pagination
concord::rate_limit
concord::records
concord::response
concord::runtime
concord::dangerous
concord::advanced
concord::prelude
```

The domain modules reexport core and extras types without wrappers, preserving
type identity and semantic ownership. Traits needed by downstream codec, body,
response, provider, pagination, limiter, observer, hook, and record
implementations are available from their domains. `dangerous` remains
feature-gated. `advanced` remains a compatibility aggregation, not the sole
discoverable home of any domain. The façade exposes no broad `core` or
`extras` module, and `__private` remains rustdoc-hidden.

Rendered façade rustdoc contains the 15 feature-enabled public modules above,
contains no broad implementation module, and passed with warnings denied. A
downstream-style public namespace test passes with both no features and all
features.

## One-crate consumers and macro neutrality

All canonical example, example-test, and benchmark product imports now use
`concord` domain paths. Their manifests have no direct core, macros, or extras
edge. The performance harness retains only the documented
`concord::__development` bridge plus its independent deterministic
test-support dependency.

A recursive policy proof rejects `concord_core`, `concord_macros`, or
`concord_extras` names in canonical Rust consumer sources. The README quick
start now declares one `concord` dependency and relies on façade defaults.

The macro still resolves the caller-visible façade package, including a
dependency rename, and emits through its hidden bridge. The renamed-façade
expansion contains 27 `::runtime::__private` references and zero
`::concord_core` references. A supplemental scan found zero JSON, CSV, NDJSON,
OAuth2, governor, or built-in pagination-controller names in
`concord_macros/src`. The direct-core resolver branch remains only for internal
compatibility tests and is still scheduled for Phase 14 removal.

## Lifecycle and security preservation

This phase adds paths and changes feature/package ownership; it does not add
an executor, body mode, response mode, native transport escape, or alternate
authentication/limiting path. The 324-test all-feature core suite and focused
redaction matrix pass. They continue to prove inert request construction,
immutable runtime snapshots, private native auth, fail-closed generated rate
plans, one bounded recovery resend, bounded body/status/stream/pagination
acquisition, sequential all-or-error pagination, deterministic result
fixation/completion, and cancellation behavior.

The façade OAuth/provider lifecycle tests, governor composition tests,
third-party extension fixture, and extras suites also pass after migration to
domain paths. Seeded error, hook, proxy, sanitized-header, provider, and status
body proofs remain value-free.

## Packaging posture

Package descriptions and docs.rs all-feature metadata exist for the façade,
core, extras, macros, and internal test support. Implementation crate docs
direct ordinary applications to `concord`.

The registry order is:

1. `concord-test-support`, as an internal prerequisite for the explicitly
   unstable development feature and development checks;
2. `concord-core`;
3. `concord-extras` and `concord-macros`;
4. `concord`.

`concord-test-support` successfully packages and verifies locally. The four
product dry-runs do not pass yet because their exact siblings are intentionally
not present in a registry. Cargo also reports the deliberately unfinished
license/repository metadata. All packages retain `publish = false`; Phase 14
owns release metadata, archive inspection, and a dependency-order temporary
registry proof. The unrelated crates.io package already occupying `concord`
remains the release-name blocker. No publish operation was performed.

See [Packaging and publication posture](packaging-and-publication.md).

## Commands and observed results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo check --workspace --all-targets --locked` | PASS |
| `cargo check --workspace --all-targets --all-features --locked` | PASS |
| `cargo test -p concord --all-features` | PASS; façade generated surface, OAuth lifecycle, namespace, governor composition, and type-neutrality tests |
| `cargo test -p concord --all-features --locked` | PASS |
| `cargo test -p concord-examples --all-features` | PASS; canonical construction remains inert |
| `cargo check -p concord-examples --all-targets --all-features` | PASS |
| `cargo check -p concord-perf --all-targets --all-features` | PASS |
| `cargo check -p concord --all-targets --no-default-features` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features default-tls,http2,rate-limit-governor` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features json` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features records` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features csv` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features ndjson` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features oauth2-client` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features multipart` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features gzip,brotli,deflate` | PASS |
| `cargo check -p concord --all-targets --no-default-features --features dangerous-raw-response,dangerous-custom-retry` | PASS |
| `cargo test -p concord --test public_namespaces --no-default-features` | PASS; 1 downstream-style domain test |
| `cargo test -p concord --test public_namespaces --all-features` | PASS; 8 domain/feature tests |
| `cargo test -p concord-macros --test dependency_policy` | PASS; 9 graph, feature, package, and source-policy tests |
| `cargo test -p concord-macros` | PASS; 33 compiler units, 17 graph fixtures, 9 dependency-policy tests, generated runtime suites, 26 UI groups, and doctests |
| `cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-facade --all-features --offline --locked` | PASS |
| `cargo test --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --all-features --offline --locked` | PASS; third-party semantic extensions use only public namespaces |
| `cargo test -p concord-core --no-default-features --lib` | PASS; 247 tests |
| `cargo test -p concord-core --lib --all-features` | PASS; 324 lifecycle/security/boundedness tests |
| `cargo test -p concord-core --no-default-features` | FAIL only in the three recorded Rust 1.97.1 dangerous-feature Trybuild rendering mismatches; preceding runtime/integration tests pass and no snapshot was blessed |
| `cargo test -p concord-extras --no-default-features` | PASS |
| `cargo test -p concord-extras --all-features` | PASS; 37 concrete implementation tests, public controller/governor tests, and doctest |
| focused `deterministic_redaction`, `error_redaction`, `hook_redaction`, `proxy`, `sanitized_metadata`, and `status_body_contract` core integration tests with all features | PASS; 17 seeded security/boundedness tests |
| `cargo test --workspace --all-features` | FAIL only in the eight Phase 0 Rust 1.97.1 core UI rendering mismatches after the ordinary workspace runtime tests reached by the command pass; no snapshot was blessed |
| `cargo test --workspace --doc --all-features` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord-core --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord-extras --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord-macros --no-deps` | PASS |
| `cargo +1.85.0 check --workspace --all-targets` | PASS |
| `cargo +1.85.0 test -p concord --no-default-features` | PASS |
| `cargo +1.85.0 test -p concord-core --no-default-features` | PASS, including privacy/UI and fixture proofs |
| `cargo +1.85.0 test -p concord-extras --no-default-features` | PASS |
| `cargo tree -p concord --all-features` | PASS |
| `cargo metadata --format-version 1 --all-features` | PASS; 7 workspace packages |
| supplemental macro concrete-extra-name scan | PASS; zero matches |
| `cargo package -p concord-test-support --allow-dirty` | PASS; 12 files, 82.0 KiB uncompressed, 21.7 KiB compressed; warns that Phase 14 release metadata is absent |
| `cargo package -p concord-core --allow-dirty` | FAIL/DEFERRED; exact `concord-test-support` is not in a registry |
| `cargo package -p concord-extras --allow-dirty` | FAIL/DEFERRED; exact `concord-core` is not in a registry |
| `cargo package -p concord-macros --allow-dirty` | FAIL/DEFERRED; exact `concord-core` development dependency is not in a registry |
| `cargo package -p concord --allow-dirty` | FAIL/DEFERRED; exact `concord-core` is not in a registry |
| `cargo deny check` | PASS; retains the reviewed duplicate `core-foundation` and `windows-sys` warnings |
| `cargo bench -p concord-perf --bench concord_overhead --no-run` | PASS |
| `cargo bench -p concord-perf --bench concord_overhead -- --smoke` | PASS; all 11 named operations executed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to installed Rust 1.97.1 |
| `git diff --check` | PASS |

An exploratory `cargo check` of every binary in the independent
`config_features` fixture without selecting its feature-specific harness
failed as designed because that package contains mutually exclusive
presence/absence programs. Its lockfile changed only for local package names;
the owning feature harness subsequently passed in the complete Rust 1.85 core
test.

## Feature cost and expansion measurements

On the recorded Windows target, the façade normal dependency tree contains 85
unique packages with no defaults and 99 with the selected defaults. The 14
default-only packages are:

```text
cfg-if 1.0.4
fnv 1.0.7
getrandom 0.2.17
h2 0.4.15
hyper-rustls 0.27.9
ring 0.17.14
rustls 0.23.37
rustls-pki-types 1.15.0
rustls-platform-verifier 0.7.0
rustls-webpki 0.103.13
subtle 2.6.1
tokio-rustls 0.26.4
untrusted 0.9.0
zeroize 1.9.0
```

Governor composition adds implementation code but no additional unique
package on this graph because its HTTP/date/Tokio dependencies are already
present in core's normal graph. HTTP/2 and the TLS stack account for the
default-only packages.

The internal macro generated-surface expansion remains 7,419 lines. The
caller-renamed façade fixture expands to 2,563 lines and 151,063 characters,
with 27 façade bridge paths and zero direct core paths.

The Phase 5 benchmark smoke observed one iteration per named operation:

```text
core client construction                         235.4 µs
generated client/facade/request creation           4.3 µs
public head preparation                           50.1 µs
codec encode/decode                                2.3 µs
body-limit polling                              3.1955 ms
ready credential preparation/finalization         44.6 µs
ready limiter acquisition                       2.3883 ms
pagination loop                                 2.5769 ms
stream wrapping                                    800 ns
generic specification selection                  15.8 µs
record framing                                      500 ns
```

These are local observational samples, not product thresholds.

## Validation-driven corrections and bounded limitations

Validation found and fixed two proof/surface mistakes:

- `RateLimitErrorKind` belongs to core's error export and is reexported from
  the façade rate-limit domain through that identity-preserving path.
- The core generated-contract integration test still asserted bridge version
  17 after Phase 4 established version 18. The stale assertion was updated;
  the bridge itself did not change.

The three dangerous-feature and eight complete-core Rust 1.97.1 Trybuild
rendering mismatches remain the Phase 0 compiler-output blocker. Clippy remains
unavailable. Product package archives remain a Phase 14 registry-order proof,
and manual rust-analyzer interaction remains a release gate. No selected
architecture decision required a deviation.
