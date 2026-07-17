# CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate  
**Authority:** Crate topology, Cargo features, MSRV, dependency baseline, stability, and build compatibility  
**Requirement prefixes:** `FEAT`, `COMPAT`  
**Depends on:** Specification Index, Product Contract, Public API Specification, Security and Limits Specification, Execution Specification  
**Supersedes:** None

## 1. Workspace crates

The Concord 0.1 workspace contains:

```text
concord_core
concord_macros
concord_examples
```

**FEAT-WORKSPACE-001 — Optional test-support crate.** A `concord_test_support` crate MAY be added for downstream deterministic tests without changing product behavior.

### 1.1 `concord_core`

Owns the runtime and public Rust APIs.

### 1.2 `concord_macros`

Owns DSL parsing, semantic resolution, normalized IR, and generated Rust.

### 1.3 `concord_examples`

Contains compile-checked and deterministic examples. Normal automated examples perform no live network request.

**COMPAT-CRATE-001 — Macro/runtime separation.** `concord_macros` MUST NOT depend on Reqwest, Tokio, Governor, Serde JSON, or other runtime-only implementation crates.

## 2. Rust and dependency baseline

**COMPAT-RUST-001 — Edition.** Concord 0.1 uses Rust edition 2024.

**COMPAT-RUST-002 — MSRV.** The minimum supported Rust version is Rust 1.85.

The MSRV is selected from the Rust 2024 language floor and the declared dependency baseline, then enforced in CI. It is not automatically raised to the newest stable compiler.

**COMPAT-REQWEST-001 — Reqwest baseline.** Concord 0.1 targets Reqwest 0.13.4.

Reqwest default features are disabled, and required capabilities are enabled explicitly through Concord features.

A Reqwest upgrade that changes the dangerous retry builder type, retry defaults, body behavior, TLS behavior, or public error integration requires compatibility review and a specification version update where necessary.

**COMPAT-REQWEST-002 — Dangerous retry coupling.** The `dangerous-custom-retry` surface is intentionally coupled to the selected Reqwest baseline and MAY require a breaking Concord minor release when Reqwest changes that native type.

**COMPAT-REQWEST-003 — Retry-disabled Concord baseline.** Concord application clients MUST disable native Reqwest retry unless the DSL or safe HTTP builder explicitly enables Reqwest default or dangerous custom retry.

## 3. Core feature table

| Feature | Default | Capability |
|---|---:|---|
| `default-tls` | Yes | HTTPS and safe TLS root/identity configuration |
| `http2` | Yes | Reqwest HTTP/2 support and reviewed HTTP/2 configuration |
| `rate-limit-governor` | Yes | Built-in process-local governor limiter |
| `json` | No | `Json<T>` request/response codecs and built-in OAuth2 client credentials |
| `multipart` | No | Request-side `MultipartBody` and DSL `Multipart` |
| `gzip` | No | Reqwest gzip response decompression |
| `brotli` | No | Reqwest Brotli response decompression |
| `deflate` | No | Reqwest Deflate response decompression |
| `dangerous-raw-response` | No | Successful bounded raw response terminal |
| `dangerous-custom-retry` | No | Reqwest retry builder re-export and custom retry DSL/configuration |
| `dangerous-dev-tools` | No | Downstream deterministic execution test seam |

**FEAT-CORE-001 — Default feature set.** The default core feature set MUST be exactly `default-tls`, `http2`, and `rate-limit-governor`.

**FEAT-CORE-002 — Mandatory Reqwest.** Reqwest MUST remain a mandatory dependency in every `concord_core` configuration.

## 4. Default build

A default build provides:

- HTTP and HTTPS for unauthenticated application requests and HTTPS for protected requests;
- application native retry disabled unless explicitly enabled;
- HTTP/1 and HTTP/2 capabilities selected by Reqwest negotiation;
- process-local governor rate limiting;
- text, bytes, no-content, and stream response modes;
- custom codecs;
- custom providers;
- custom pagination;
- custom limiters and hooks.

It does not provide:

- JSON;
- multipart;
- response decompression features listed above;
- dangerous raw response;
- dangerous custom retry;
- downstream development tools.

## 5. No-default build

`concord_core --no-default-features` remains a production Reqwest runtime.

It provides:

- unauthenticated HTTP;
- application native retry disabled unless explicitly enabled;
- concrete managed Reqwest clients;
- text, bytes, no-content, and stream support;
- custom codecs;
- custom credential providers;
- custom pagination;
- custom limiter contracts;
- hooks and safe HTTP configuration applicable to available capabilities.

It does not provide:

- TLS/HTTPS;
- HTTP/2-specific configuration;
- built-in governor enforcement.

A fixed HTTPS generated client fails construction without TLS.

A dynamic HTTPS request fails during public request preparation before body, hook, provider, limiter, or transport side effects. A protected request cannot fall back to HTTP, and provider HTTP is unavailable without TLS.

**FEAT-NODEFAULT-001 — HTTP remains functional.** A no-default build MUST be able to execute HTTP through the managed Reqwest runtime.

**FEAT-NODEFAULT-002 — No downgrade.** Missing TLS MUST produce a typed failure and MUST NOT downgrade HTTPS to HTTP.

## 6. JSON feature

`json` enables:

- public `Json<T>` marker;
- Serde request encoding;
- Serde response decoding;
- generated JSON DSL support;
- built-in OAuth2 client-credentials provider.

It does not enable TLS, HTTP/2, governor, compression, or multipart.

**FEAT-JSON-001 — Independent JSON.** `--no-default-features --features json` MUST enable JSON without enabling unrelated default capabilities.

## 7. Multipart feature

`multipart` enables:

- public `MultipartBody`;
- request-side multipart part APIs;
- DSL `body: Multipart`;
- Reqwest multipart integration.

It does not enable response multipart parsing.

**FEAT-MULTIPART-001 — Request-only multipart.** The multipart feature MUST add no multipart response mode.

## 8. Compression features

`gzip`, `brotli`, and `deflate` independently enable corresponding Reqwest response decompression.

When enabled, response limits count bytes after decompression as required by `BOUND-BODY-*`.

Each feature exposes only its reviewed safe configuration methods.

**FEAT-COMPRESS-001 — Independent decompression.** Enabling one decompression feature MUST NOT enable another.

## 9. HTTP/2 feature

`http2` enables Reqwest HTTP/2 support and HTTP/2-specific safe configuration, including prior knowledge where supported.

The `concord_core` and `concord_macros` normal/build graphs do not directly depend on `h2`.
`concord_core` may retain an isolated dev-only `h2` dependency for deterministic Reqwest
upstream-contract probes such as `REFUSED_STREAM`. That test exception does not enter published
runtime code, generated code, public APIs, or normal/build dependency graphs.

## 10. Governor feature

`rate-limit-governor` enables `GovernorRateLimiter` as the generated default functional limiter.

Without it:

- public rate-limit types and traits remain available;
- the built-in fallback limiter authorizes an empty effective plan and rejects a non-empty effective plan before send;
- generated clients statically known to declare or attach rate-limit rules require a custom functional limiter or explicit `NoopRateLimiter` during construction;
- construction fails otherwise;
- an advanced prepared endpoint that introduces a non-empty plan on a client with the fallback limiter fails at limiter acquisition rather than silently proceeding.

**FEAT-GOV-001 — Fail-closed absence.** Disabling the governor MUST NOT silently turn declared rate limits into no-op behavior.

**FEAT-GOV-002 — Advanced-plan fallback.** Without the governor or an explicitly installed limiter, a dynamically introduced non-empty prepared-endpoint plan MUST fail before native send while an empty plan remains executable.

## 11. Dangerous raw response

`dangerous-raw-response` enables:

- public dangerous `RawResponse`;
- generated `.raw_response()` terminals.

Without it, those items and methods are absent.

The feature does not enable dangerous retry or development tools.

## 12. Dangerous custom retry

`dangerous-custom-retry` enables:

- `concord_core::dangerous::ReqwestRetryBuilder`;
- DSL `retry custom PATH`;
- reviewed dangerous runtime builder-taking retry configuration.

It does not enable:

- raw responses;
- development tools;
- provider custom retry;
- raw Reqwest clients;
- redirects;
- cookies;
- unsafe TLS.

**FEAT-DANGER-001 — Dangerous independence.** Each dangerous feature MUST be independently selectable.

**FEAT-DANGER-002 — Default absence.** No dangerous feature may be enabled by default.

## 13. Dangerous development tools

`dangerous-dev-tools` enables downstream access to `concord_core::__development`.

The module is:

- absent without the feature for downstream code;
- unstable;
- not in `prelude`, `advanced`, or `dangerous`;
- not referenced by generated client type signatures;
- not enabled merely by debug assertions or a development profile.

Core unit tests may compile equivalent private support under `cfg(test)`.

**FEAT-DEV-001 — No production generic.** Development execution support MUST NOT add an executor type parameter or constructor argument to generated production clients.

## 14. Macro feature model

`concord_macros` has no runtime feature matrix in 0.1.

The parser recognizes feature-dependent syntax, and generated Rust produces focused compile-time errors when the corresponding core capability is absent.

The macro crate does not duplicate core features.

**COMPAT-MACRO-001 — Single feature authority.** Runtime capability feature selection MUST be owned by `concord_core`, not mirrored by unrelated macro features.

### 14.1 Generated core-path resolution

Generated Rust must resolve the caller-visible `concord_core` dependency path rather than hardcode `::concord_core`. The macro uses package-aware crate-path discovery, such as `proc_macro_crate`, and emits one collision-safe private alias per macro expansion.

The contract covers:

- ordinary dependency names;
- renamed dependencies;
- workspace dependency aliases;
- macro invocation inside nested modules;
- multiple API declarations and multiple macro invocations in one crate.

A caller-provided path override may be added only through a later DSL contract revision.

**COMPAT-MACRO-002 — Renamed dependency support.** Generated code MUST resolve and use the caller-visible core crate path and MUST compile when the `concord_core` package is renamed in `Cargo.toml`.

**COMPAT-MACRO-003 — Expansion isolation.** Nested and repeated macro expansions MUST use collision-safe internal aliases and MUST NOT depend on invocation at crate root.

## 15. Feature-gated diagnostics

Disabled feature use must fail during compilation where statically knowable.

Examples:

- `Json<T>` without `json`;
- OAuth2 client credentials without `json`;
- `Multipart` without `multipart`;
- `.raw_response()` without `dangerous-raw-response`;
- `retry custom` without `dangerous-custom-retry`;
- `__development` without `dangerous-dev-tools`.

Diagnostics identify the exact required feature and do not expose application values.

**FEAT-DIAG-001 — No silent fallback.** Feature-gated syntax or methods MUST NOT silently use a different body, response, retry, or limiter behavior.

## 16. Feature independence matrix

| Enabled | Does not automatically enable |
|---|---|
| `json` | TLS, HTTP/2, governor, compression, multipart |
| `multipart` | JSON, compression, dangerous features |
| `gzip` | Brotli, Deflate, JSON, multipart |
| `dangerous-raw-response` | dangerous retry, development tools |
| `dangerous-custom-retry` | raw response, development tools |
| `dangerous-dev-tools` | raw response, dangerous retry, TLS |

## 17. Cargo feature unification

Cargo features are unified per crate in one resolved dependency graph.

Enabling a dangerous feature through any dependency path makes that dangerous API available in the compiled `concord_core` crate.

Applications are responsible for auditing their resolved feature graph.

**COMPAT-FEATURE-001 — Binary-wide capability.** Concord documentation MUST describe dangerous feature enablement as a compiled-crate capability, not a per-client setting.

## 18. Generated macro/core compatibility

Every generated client module performs a compile-time compatibility assertion against a semantic core-generated integration marker.

Requirements:

- compatible macro/core generated contracts compile;
- incompatible generated contracts fail at compile time;
- the check executes no runtime code;
- package-version string equality is neither required nor sufficient;
- the exact private marker name is not a public API.

**COMPAT-GEN-001 — Compile-time contract marker.** Incompatible macro/core generated integration contracts MUST fail during compilation.

**COMPAT-GEN-002 — No runtime check.** Generated compatibility checking MUST have no runtime cost.

## 19. Stability surfaces and pre-1.0 policy

Concord 0.x is a pre-1.0 project.

### 19.1 Patch releases

Within one `0.y.z` minor line, patch releases preserve documented normal and advanced public signatures and generated-contract compatibility except for security fixes whose incompatibility is explicitly documented.

### 19.2 Minor releases

A new `0.(y+1).0` release may contain documented breaking changes to the DSL, generated API, stable advanced traits, or observable behavior. Such a change updates the generated contract marker and migration/release notes.

### 19.3 Dangerous surface

Dangerous APIs are feature-gated and follow the same patch/minor policy for signatures, but their documented behavioral guarantees are intentionally reduced. The native Reqwest retry type is dependency-coupled as specified by `COMPAT-REQWEST-002`.

### 19.4 Unstable integration surfaces

`__private` may change whenever the macro/core generated contract marker changes. `__development` may change without normal semver compatibility and is not a production API.

**COMPAT-SURF-001 — Public extension sufficiency.** Every documented stable extension workflow MUST be implementable without importing `__private` or `__development`.

**COMPAT-SURF-002 — Pre-1.0 break policy.** Breaking normal or advanced API changes MUST occur in a documented Concord minor release, not silently in a patch release.

## 20. Dependency policy

**COMPAT-DEP-001 — Prohibited transport-layer dependencies.** The `concord_core` and `concord_macros` normal and build dependency graphs MUST NOT add direct dependencies on:

- `hyper`;
- `hyper-util`;
- `tower`;
- `h2`.

Reqwest owns those production transport details. A direct test-only `h2` dev dependency MAY be
used solely in isolated deterministic upstream-contract probes. It MUST NOT appear in published
runtime code, generated code, public APIs, or either package's normal/build graph.

**COMPAT-DEP-002 — Explicit dependency versions.** Workspace dependencies MUST use explicit compatible version requirements; wildcard dependencies are prohibited.

Git and non-default registry dependencies require an explicit recorded decision.

**COMPAT-DEPS-001 — Reqwest ownership.** Concord production and build graphs MUST NOT duplicate Reqwest's underlying HTTP stack through direct transport-stack dependencies.

## 21. Required build matrix

A release candidate must compile and test at least:

| Configuration | Required checks |
|---|---|
| Core defaults | check, Clippy, tests, docs |
| Core all features | check, Clippy, tests, docs |
| Core no default | check and focused tests |
| Core no default + `json` | check and focused JSON tests |
| Core each dangerous feature independently | compile and boundary tests |
| Macro crate | check, Clippy, unit/trybuild tests |
| Public extension fixture | current compiler and MSRV |
| Examples | compile and deterministic tests |
| Workspace MSRV | required crates and public fixture |

Exact CI commands are owned by the Conformance Plan.

**COMPAT-BUILD-001 — MSRV evidence.** Core, macro, generated API fixtures, and public extension fixtures MUST compile on the declared MSRV.

## 22. Documentation annotations

Every feature-gated public example must identify its required feature in prose or Cargo configuration.

Generated Rustdoc for a gated method must name the gate.

**FEAT-DOC-001 — Gate visibility.** Public documentation MUST NOT present a feature-gated capability as universally available.
