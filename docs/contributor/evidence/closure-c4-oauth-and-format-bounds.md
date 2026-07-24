# Closure C4 OAuth2 and formatted-capture bounds

Phase:
Closure C4 — Bound OAuth2 and all Concord-owned formatted capture.

Started from commit:
`12a71f612dd0bfd2ccb877f401067a2bd33ad2f5`
(`core: enforce finite rate limit structures`) on branch `master`.

Finished at commit:
The C4 commit containing this evidence, with message
`extras: bound OAuth2 configuration and encoding`. Its exact hash will be
backfilled in the closure ledger by C5.

Files changed:

- `CHANGELOG.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `MIGRATION.md`;
- `crates/concord-core/src/auth/provider.rs`;
- `crates/concord-core/src/execute/auth_tests.rs`;
- `crates/concord-core/src/execute/tests.rs`;
- `crates/concord-core/src/http/provider.rs`;
- `crates/concord-core/src/pagination/runtime_tests.rs`;
- `crates/concord-core/src/request/head.rs`;
- `crates/concord-core/src/request/prepared.rs`;
- `crates/concord-extras/src/auth/mod.rs`;
- `crates/concord-extras/src/auth/oauth2_client.rs`;
- `crates/concord-macros/src/emit/shaping.rs`;
- `crates/concord/src/lib.rs`;
- `crates/concord/tests/oauth2_provider.rs`;
- `crates/concord/tests/public_extensions.rs`;
- `crates/concord/tests/public_namespaces.rs`;
- `crates/concord/tests/rustdoc.rs`;
- `docs/contributor/evidence/closure-c3-rate-limit-bounds.md`;
- `docs/contributor/evidence/closure-c4-oauth-and-format-bounds.md`;
- `docs/guide/04-authentication.md`;
- `docs/reference/limits.md`.

Design decisions:

- `ProviderHttp::request_body_limit()` returns the active `NonZeroUsize`
  request-body ceiling and exposes no native client, pool, TLS, or request
  builder. `CredentialRequest` documents this pre-allocation provider use.
- OAuth2 remains entirely extras-owned. Its public finite limits are 8,192
  bytes each for token URL text, client ID, and client secret; 65,536 bytes
  for scope; and 32,768 bytes for the final Basic Authorization value. The URL
  cap bounds parsing, the credential caps generously exceed ordinary protocol
  inputs, and the independent 32 KiB header cap prevents worst-case
  percent/Base64 expansion from inheriting the larger raw aggregate.
- The fluent OAuth builder latches the first value-free failure. Borrowed
  `AsRef<str>` setters inspect length before copying; `client_id_owned` and
  `client_secret_owned` validate and retain an existing `SecretString`
  without another protected copy. Oversized candidates and later setter
  inputs are not retained after failure.
- New build-error kinds distinguish an overlong URL, client ID, client secret,
  and scope from the existing unsafe-URL and missing-field categories. None
  retains or renders source text.
- OAuth exchange makes a checked length pass before output allocation.
  `url::form_urlencoded::byte_serialize` supplies exact form components without
  intermediate strings. A streaming Base64 sink consumes those components and
  the separator directly into one exactly reserved byte vector.
  `Bytes` and `HeaderValue::from_maybe_shared` adopt the final backing
  allocations. There is no complete encoded username, encoded password,
  combined credential, Base64 string, or formatted Basic string.
- Form-body length is checked against the actual provider capability before
  either final output is allocated. Arithmetic or allocation failure becomes
  the existing safe invalid-configuration category; body excess becomes the
  existing request-too-large category.
- Every dynamic generated/public-head host, safe host, path, query, public
  header, and pagination-sensitive value now uses C1's one capped formatter.
  Generated query emission propagates its new safe result. Formatting occurs
  before URL/header mutation, so first excess is rejected rather than
  truncated or retained.
- The product-source `Display::to_string()` inventory found no remaining
  ordinary request-shaping capture in `HeadBuilder`, runtime endpoint
  preparation, or OAuth2. The remaining production `Url::to_string()` renders
  an already structured, bounded safe URL rather than arbitrary user
  `Display`; private debug capture converts already-sanitized arguments only
  under `cfg(test)`, while production debug output streams to stderr. Private
  authentication materialization remains the separately documented
  credential-extension trust boundary, not public-head capture.
- User `Display` implementations may still consume CPU, block, panic, or
  ignore `fmt::Error`. Tests prove that ignoring the error cannot make Concord
  accept or retain excess output.

Audit findings closed:

- F-08 is closed. First-party OAuth2 configuration is finite, URL parsing and
  field copying follow raw bounds, encoded lengths are checked, final outputs
  each use one backing allocation, and the provider runtime ceiling applies
  before form allocation or transport.
- F-18 is closed. Runtime endpoint formatting was completed in C2; C4 routes
  all remaining generated/public-head and pagination-shaped dynamic capture
  through the same hard cap and records the non-capture trust boundaries.

Tests added/changed:

- Added every required OAuth owner:
  `oauth_token_url_max_and_first_excess`,
  `oauth_client_id_max_and_first_excess`,
  `oauth_client_secret_max_and_first_excess`,
  `oauth_scope_max_and_first_excess`,
  `oauth_encoded_authorization_checked_before_allocation`,
  `oauth_form_body_checked_against_provider_limit`,
  `oauth_boundary_request_is_protocol_correct`,
  `oauth_first_excess_errors_are_secret_safe`, and
  `oauth_debug_and_display_remain_redacted`.
- Added every required head owner:
  `generated_path_formatted_value_first_excess`,
  `generated_query_formatted_value_first_excess`,
  `generated_header_formatted_value_first_excess`,
  `pagination_formatted_value_first_excess`, and
  `bounded_formatter_does_not_retain_protected_sentinel`.
- A formatter probe writes exactly the cap and then a protected sentinel
  chunk. Path/query/header/host and pagination tests prove no mutation and no
  safe error rendering of that chunk, including when `Display` ignores the
  formatter error.
- The façade OAuth lifecycle owner now configures a one-byte provider request
  limit and proves form rejection occurs with zero loopback provider requests.
- Core proves the capability returns its configured limit. The public
  extension owner calls the accessor through façade-only imports, and the
  namespace owner checks all five OAuth constants.
- Macro query emission, hand-authored endpoint tests, pagination endpoints,
  runtime endpoint preparation, and rendered Rustdoc expectations were
  updated for fallible bounded query capture and the new provider contract.

Commands executed:

```text
cargo fmt --all
cargo fmt --all -- --check
cargo check -p concord-core --lib --all-features --locked
cargo check -p concord-extras --all-targets --all-features --locked
cargo check -p concord --tests --all-features --locked
cargo test -p concord-core --lib --all-features --locked request::head::tests
cargo test -p concord-extras --lib --all-features --locked oauth2_client::tests
cargo +stable clippy -p concord-core -p concord-extras -p concord -p concord-macros --all-targets --all-features --locked -- -D warnings
cargo test -p concord-core --lib --all-features --locked provider_http_exposes_the_active_request_body_limit -- --exact
cargo test -p concord-core --lib --all-features --locked http::provider::tests::provider_http_exposes_the_active_request_body_limit -- --exact
cargo test -p concord --test public_namespaces --all-features --locked
cargo test -p concord --test public_extensions --all-features --locked
cargo run --locked -p concord-xtask -- docs
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo test -p concord --test rustdoc --all-features --locked
cargo +1.85.0 check -p concord-core -p concord-extras -p concord-macros -p concord --all-targets --all-features --locked
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord --test generated_authentication --all-features --locked
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test generated_lifecycle --all-features --locked
cargo test -p concord --test oauth2_provider --all-features --locked
cargo test -p concord --all-features --locked
cargo test -p concord-examples --all-features --locked
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo test -p concord-macros --lib --locked
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
git diff --check
git status --short
```

Focused commands and the Clippy command were repeated after their documented
repairs. Read-only inspection also used `rg`, `Get-Content`, `git diff`,
`git diff --stat`, and `git status --short` to inspect the authoritative C4
requirements, provider/OAuth ownership, all request-shaping formatting paths,
call sites, documentation, and the exact change set.

Exact results:

- The initial core check exited 0 with eight warnings because the first head
  implementation explicitly dropped copyable formatter errors. The mappings
  were changed to discard values without `drop`; subsequent checks are
  warning-free.
- Focused head tests exited 0 with 16 passed, including all five new formatted
  capture owners. The first focused run exposed one unused test import; it was
  removed before the full core run.
- Focused OAuth tests exited 0 with 13 passed, including all nine required
  owners.
- The first warning-denied Clippy run exited 1: three closures explicitly
  dropped non-`Drop` validation errors and one test had a needless generic
  borrow. Those values are now discarded with `_`, the borrow was removed,
  and the final stable Clippy command exited 0. The localized MSVC
  `linker_messages` warning remained visible and is explicitly exempt from
  `-D warnings`.
- One exact focused provider test command exited 0 but matched zero tests
  because it omitted the module path. The corrected exact command ran one
  test and passed.
- Core lib exited 0 with 360 passed; bounds exited 0 with 12 passed; security
  exited 0 with 3 passed; lifecycle exited 0 with 6 passed.
- Extras exited 0 with 46 unit tests, all seven integration tests, and its
  compile-fail doctest passed.
- Generated authentication exited 0 with 5 passed; endpoint planning exited 0
  with 5 passed; generated lifecycle exited 0 with 9 passed.
- The first OAuth façade run exited 0 with its original 3 tests. After adding
  the runtime request-limit/zero-transport proof, the final OAuth owner exited
  0 with 4 passed.
- Public namespaces exited 0 with 9 passed; public extensions exited 0 with 3
  passed.
- The complete all-feature façade suite exited 0. All integration owners,
  7 public-compile cases, 7 stable exact UI groups, 2 rendered Rustdoc tests,
  and doctests passed; no Trybuild snapshot changed.
- Macro unit tests exited 0 with 65 passed.
- Examples exited 0: 1 construction test, 7 local-runtime tests, and 3 Riot
  contract tests passed; all targets and doctests passed.
- Rust 1.85 all-target/all-feature compilation for core, extras, macros, and
  façade exited 0.
- Documentation ownership and links exited 0. Warning-denied façade, extras,
  and core Rustdoc each exited 0; rendered Rustdoc exited 0 with 2 passed.
- The optimized benchmark no-run command exited 0 and produced
  `concord_overhead-d01aa542161bf3e9.exe`.
- Final format, documentation, Clippy, diff, and status checks are rerun
  immediately before the phase commit.
- Two ancillary discovery incidents did not change source: a recursive Cargo
  registry search timed out after locating the needed form-encoding source,
  and one attempted `Get-Content` used a nonexistent `auth/secret.rs` path
  before the correct `auth/material.rs` owner was read. One broad documentation
  search exited 1 only because an optional top-level `examples` directory does
  not exist; all existing matches were returned.

Unexecuted checks:

- No benchmark executable was run, so no numeric performance comparison is
  claimed.
- No graphical editor or manual rust-analyzer acceptance session ran in C4.
- No remote GitHub Actions or independent Linux/macOS platform lane ran.
- No package, registry, legal-metadata, or publication check ran in C4.
- No external network endpoint was used. OAuth runtime tests used only the
  deterministic loopback TLS owner.

Security/redaction review:

- Builder failures retain only a stable enum. The URL, client identifier,
  secret, scope, encoded credential, final header, form body, and allocation
  errors never enter `Debug`, `Display`, or an error source.
- Raw field first-excess tests and encoded header/body failures use distinct
  sentinels and inspect both `Display` and `Debug`. Provider-limit integration
  also walks the complete safe source chain and observes zero transport.
- Both final output allocations occur only after every checked length and
  configured limit comparison. The Authorization backing remains protected
  and enters only the isolated provider request; the form body remains outside
  application hooks, limiting, transport, completion, and diagnostics.
- Head formatting failures map only to existing route/header configuration
  kinds. Safe and pagination-protected builders remain unchanged after
  failure, and neither logical nor safe prepared heads contain rejected text.
- Full authentication, lifecycle, security, redaction, planning, pagination,
  stable UI, façade, and example owners passed. Native transport privacy,
  one-additional-attempt recovery, first-excess withholding, completion,
  cancellation, and response acquisition were unchanged.

Performance impact:

- OAuth length validation makes one allocation-free pass over bounded
  credential/scope input and one encoding pass. Each final header/body has one
  exact backing allocation; the former five full-size credential/header
  intermediates and serializer body intermediate are gone.
- Generated dynamic shaping replaces unbounded `to_string()` with the capped
  writer. Successful values still require one retained string before URL or
  header validation, now capped at 65,536 bytes.
- The optimized benchmark target compiles. No measured regression or
  improvement is claimed.

Deviations:
None.

Remaining blockers:

- C5 must remove the panic-prone endpoint ID conversion, rename the
  runtime-only timeout setting, normalize macro syntax into semantic enums,
  remove obsolete bridge helpers, and mechanically own exact documented
  limits.
- C6–C8 remain in their planned order for package-graph repair, executable
  validation/documentation truth, and final certification.
