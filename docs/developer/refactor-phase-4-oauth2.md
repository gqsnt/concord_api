# Phase 4 OAuth2 extraction and generic provider validation

Phase 4 was implemented from commit `2ed0c43` with Rust and Cargo 1.97.1.
MSRV checks used Cargo 1.85.0.

## Resulting ownership and public contract

`OAuth2ClientCredentials` now lives only in `concord_extras::auth`. It
implements the public core `CredentialProvider` contract with
`Material = AccessToken` and uses only the bounded semantic `ProviderHttp`
capability exposed through `CredentialRequest`. Extras receives no native
Reqwest request, response, application executor, or authentication boundary
object.

The provider has a named, secret-safe builder:

```rust
let provider = concord::auth::OAuth2ClientCredentials::builder(token_url)
    .client_id(client_id)
    .client_secret(client_secret)
    .scope("records.read")
    .build()?;

let client = Api::builder().oauth_provider(provider).build()?;
```

The builder requires an absolute HTTPS URL with a host, rejects user
information and fragments, and requires both client identifier and secret.
Invalid URL text is discarded rather than retained in an error. Builder,
provider, and build-error formatting contains only structural state.

Core retains credential materials, slots, challenge policy, provider HTTP,
finite request/response acquisition, recovery ordering, attempt bounds,
private native materialization, and the executor. Its concrete OAuth module,
feature, hidden factories, feature assertion, placeholder provider, and normal
Serde JSON dependency are gone.

The façade owns `oauth2-client` forwarding and reexports the provider under
`concord::auth`. The macro now parses:

```rust
credential oauth = custom OAuth2ClientCredentials;
```

as an opaque provider type. Generated storage and placement derive the
material as `<Provider as CredentialProvider>::Material`. Named provider
setters accept any provider with that material, and the generated custom
credential handle exposes the same-material `replace_provider`. Aliased,
fully-qualified, and local third-party providers use the same path. The old
`oauth2_client` grammar, JSON/OAuth feature assertions, source-specific
factories, positional OAuth secrets, and special replacement handle were
removed rather than retained as a second model.

The generated bridge contract advanced from 17 to 18.

## Lifecycle, boundedness, and security evidence

Deterministic TLS tests prove the extras provider emits exactly one isolated
`POST`, applies form encoding before Basic base64, uses
`application/x-www-form-urlencoded`, emits the expected client-credentials
form, decodes bearer tokens, records expiry safely, and performs refresh as
reacquisition. A generated client test proves provider acquisition precedes
limiter acquisition, an explicit limiter still wins, application transport is
not reached, and generic provider replacement clears old material without
performing eager acquisition.

Known and first-excess provider response limits remain core-owned. An
oversized response fails before limiter or application transport work and does
not expose its body. Seeded URL query values, raw and encoded client
credentials, scope, acquired tokens, replacement tokens, and provider response
bodies were checked across builder/provider `Debug`, build errors, public API
error `Display`/`Debug`, and public error source chains. The complete
all-feature core lifecycle suite separately proves hook/completion redaction,
private native auth application, provider HTTP isolation, at-most-one recovery
resend, ordering, cancellation, and finite acquisition.

Construction of providers, clients, endpoints, and requests remains inert.
The provider's body and Basic header are created only when core begins
credential acquisition.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo test -p concord_extras --features oauth2-client` | PASS; 6 unit tests, the public pagination integration test, and doctests |
| `cargo test -p concord_extras --all-features` | PASS; 37 unit tests, 2 public integration tests, and the governor privacy doctest |
| `cargo test -p concord --features oauth2-client` | PASS; façade construction, OAuth lifecycle, provider neutrality/replacement, boundedness, governor composition, and doctests |
| `cargo test -p concord --all-features` | PASS |
| `cargo test -p concord --all-features --test oauth2_provider` | PASS; 3 deterministic façade/provider lifecycle and security tests |
| `cargo check -p concord_core --no-default-features --all-targets` | PASS |
| `cargo test -p concord_core --no-default-features --lib` | PASS; 247 tests |
| `cargo test -p concord_core --no-default-features` | FAIL only in the three pre-existing Rust 1.97.1 dangerous-feature Trybuild rendering mismatches after 247 unit tests and preceding integration tests passed; no snapshot was blessed |
| `cargo test -p concord_core --lib --all-features` | PASS; 324 lifecycle, authentication, provider HTTP, boundedness, pagination, rate-limit, cancellation, and redaction tests |
| `cargo test -p concord_core --test deterministic_redaction --all-features` | PASS |
| `cargo test -p concord_macros` | PASS; 33 compiler units, 17 graph fixtures, 7 dependency-policy tests, all generated runtime suites, 26 UI tests, and doctests |
| `cargo check -p concord_extras --no-default-features --all-targets` | PASS |
| `cargo check -p concord_extras --no-default-features --features oauth2-client --all-targets` | PASS |
| `cargo check -p concord --no-default-features --features oauth2-client --all-targets` | PASS |
| `cargo test -p concord_examples --all-features` | PASS; the canonical example construction proof remained inert |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo +1.85.0 check -p concord_core --no-default-features --all-targets` | PASS |
| `cargo +1.85.0 test -p concord_extras --features oauth2-client` | PASS |
| `cargo +1.85.0 test -p concord --no-default-features --features oauth2-client,default-tls --test oauth2_provider` | PASS; 3 tests |
| `cargo test --workspace --doc --all-features` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord_core --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord_extras --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord --all-features --no-deps` | PASS |
| `cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-facade --all-features --offline --locked` | PASS |
| `cargo test --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --all-features --offline --locked` | PASS; third-party provider, limiter, hooks, codecs, bodies, responses, records, and pagination use only public contracts |
| guarded `cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-oauth-disabled --offline --locked` | PASS as an expected compile failure; `OAuth2ClientCredentials` is ordinarily absent from `concord::auth` without the feature |
| `cargo tree -p concord_core -e normal --depth 1` | PASS; no Serde, JSON, extras, façade, macro, or concrete OAuth edge |
| `cargo tree -p concord_extras -e normal --depth 1 --no-default-features --features oauth2-client` | PASS; core plus only `bytes`, `http`, `serde_json`, and `url` |
| `cargo tree -p concord -e normal --depth 1 --no-default-features --features oauth2-client` | PASS; exactly core, extras, and macros |
| `cargo tree -p concord_macros -e normal --depth 1` | PASS; no core, extras, or façade normal edge |
| `cargo metadata --format-version 1 --all-features --no-deps` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `git diff --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to the installed 1.97.1 toolchain |

The macro dependency-policy suite additionally confirms that core has no
`oauth2-client` feature, extras owns the exact OAuth dependency set, and the
façade forwards only to extras. A supplemental implementation scan found zero
OAuth2 or JSON names/branches in `concord_macros/src`.

## Expansion and validation-driven corrections

`cargo expand -p concord_macros --test generated_surface` produced 7,419
lines, 421,898 characters, 25,692 whitespace-delimited fragments, and 578
declaration-shaped lines. The Phase 0 comparison was 7,254 / 409,352 / 25,302
/ 581, so the work through Phase 4 remains near the frozen expansion baseline
and does not add declaration-shaped repetition.

Validation exposed and fixed two omissions before completion: provider aliases
needed invocation-scope imports at each nested generated endpoint module, and
the deliberately incompatible bridge fixture still expected contract 17.
Both the focused failing commands and the complete macro suite passed after
the fixes.

Regenerating the independent fixture lockfile initially selected newer cached
transitive versions. Those unrelated changes were reverted with precise
offline Cargo downgrades; the fixture lockfile is byte-for-byte unchanged.

## Bounded limitations

The exact full core no-default test and Clippy are not reported as successful.
They retain the Phase 0 Rust 1.97.1 diagnostic-rendering and tool-component
blockers described above. Manual rust-analyzer interaction was not claimed at
this phase; it remains a release-gate proof.

No selected design decision required a deviation.
