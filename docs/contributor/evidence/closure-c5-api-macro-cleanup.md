# Closure C5 public API and macro semantic cleanup

Phase:
Closure C5 — Finish public API and macro semantic boundaries.

Started from commit:
`ab193005800cb1e114f1d03f7209896bda9b02b1`
(`extras: bound OAuth2 configuration and encoding`) on branch `master`.

Finished at commit:
The C5 commit containing this evidence, with message
`refactor: finish Concord semantic API boundaries`. Its exact hash will be
backfilled in the closure ledger by C6.

Files changed:

- `CHANGELOG.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `MIGRATION.md`;
- `crates/concord-core/src/__private.rs`;
- `crates/concord-core/src/client.rs`;
- `crates/concord-core/src/endpoint.rs`;
- `crates/concord-core/src/error/status.rs`;
- `crates/concord-core/src/error/transport.rs`;
- `crates/concord-core/src/http/application.rs`;
- `crates/concord-core/src/http/status.rs`;
- `crates/concord-core/src/lib.rs`;
- `crates/concord-core/src/records/stream.rs`;
- `crates/concord-core/src/request/head.rs`;
- `crates/concord-core/src/request/prepare.rs`;
- `crates/concord-core/src/request/tests.rs`;
- `crates/concord-core/src/response/metadata.rs`;
- `crates/concord-core/src/response/stream.rs`;
- `crates/concord-core/src/runtime/config.rs`;
- `crates/concord-core/tests/public_types.rs`;
- `crates/concord-core/tests/public_types/runtime_config.rs`;
- `crates/concord-macros/src/emit/client.rs`;
- `crates/concord-macros/src/emit/descriptions.rs`;
- `crates/concord-macros/src/emit/docs.rs`;
- `crates/concord-macros/src/emit/helpers.rs`;
- `crates/concord-macros/src/emit/mod.rs`;
- `crates/concord-macros/src/emit/shaping.rs`;
- `crates/concord-macros/src/semantic/auth.rs`;
- `crates/concord-macros/src/semantic/inheritance.rs`;
- `crates/concord-macros/src/semantic/mod.rs`;
- `crates/concord-macros/src/semantic/model.rs`;
- `crates/concord-macros/src/semantic/pagination.rs`;
- `crates/concord-macros/src/semantic/symbols.rs`;
- `crates/concord/src/lib.rs`;
- `crates/concord/tests/endpoint_planning.rs`;
- `crates/concord/tests/generated_lifecycle.rs`;
- `crates/concord/tests/generated_request_shaping.rs`;
- `crates/concord/tests/public_extensions.rs`;
- `crates/concord/tests/runtime_endpoint_lifecycle.rs`;
- `crates/concord/tests/rustdoc.rs`;
- `crates/concord/tests/ui.rs`;
- `crates/concord/tests/ui/fail/capabilities/runtime_endpoint_builder_capabilities_absent.rs`;
- `crates/concord/tests/ui/fail/capabilities/runtime_endpoint_pagination_absent.rs`;
- `crates/concord/tests/ui/fail/features/dangerous_raw_runtime_endpoint_absent.rs`;
- `crates/concord/tests/ui/fail/language/endpoint_id_into_unavailable.rs`;
- `crates/concord/tests/ui/fail/language/endpoint_id_into_unavailable.stderr`;
- `crates/concord/tests/ui/fail/privacy/runtime_endpoint_fields_private.rs`;
- `crates/concord/tests/ui/pass/language/runtime_endpoint.rs`;
- `docs/contributor/evidence/closure-c4-oauth-and-format-bounds.md`;
- `docs/contributor/evidence/closure-c5-api-macro-cleanup.md`;
- `docs/guide/03-build-a-client-and-make-requests.md`;
- `docs/recipes/production-configuration.md`;
- `docs/reference/limits.md`;
- `fixtures/dependency/renamed_facade/src/lib.rs`;
- `fixtures/features/tests/no_governor.rs`;
- `tools/perf/benches/concord_overhead.rs`;
- `tools/xtask/src/main.rs`.

Design decisions:

- `EndpointId::from_static` is now crate-private. Generated descriptions call
  one documented `#[doc(hidden)]` façade bridge, which retains const
  validation and static storage without exposing an ordinary infallible
  conversion. Public runtime construction uses `EndpointId::new`,
  `TryFrom<String>`, or `TryFrom<&str>`.
- Internal error/metadata constructors now accept an exact `EndpointId`
  instead of a generic `Into<EndpointId>`. This prevents internal call sites
  from recreating the removed panic-prone string conversion and makes each
  static or fallible identity choice explicit.
- Runtime configuration consistently names its scope
  `runtime_endpoint_timeout`; generated endpoints continue to use their
  resolved DSL timeout selection, and request-local selection retains its
  existing precedence.
- Source grammar enums are converted to six semantic owners:
  `ResolvedMethod`, `ResolvedChallengePolicy`, `ResolvedAuthPlacement`,
  `ResolvedDefault`, `ResolvedRetry`, and `ResolvedValueKind`. Emit imports
  only semantic/surface vocabulary. Syntax validation still operates on the
  source nodes before the single resolution boundary so the original spans
  and diagnostics remain intact.
- The direct-core copies of the generated raw-response and development
  macros, plus the stale core/facade multipart assertion helper, were removed.
  The façade remains the generated macro owner; the remaining core
  `__private` items are individually documented semantic bridges delegated or
  reexported by the façade.
- The limits generator now reads the endpoint-ID owner and all five OAuth2
  owners in addition to the existing core defaults/hard limits. Exact public
  limits therefore have code, Rustdoc, and mechanically checked reference
  owners.

Audit findings closed:

- F-09: the public infallible `&'static str -> EndpointId` conversion and
  public static constructor are removed; generated literals retain the
  allocation-free hidden path.
- F-10: production emit no longer imports or stores source AST enum
  vocabulary.
- F-11: the runtime-only timeout API and internal snapshot now state their
  exact scope, with generated behavior proved independent.
- F-17: unreachable direct-core generated macro copies and the obsolete
  multipart feature assertion bridge are removed.
- The documentation portion of F-16 is closed: endpoint, rate-limit, and
  OAuth2 exact ceilings are public and mechanically reflected in the limits
  reference.

Tests added/changed:

- Added a stable exact Trybuild failure for the formerly panic-prone
  `"invalid identity".into()` conversion. The new E0277 snapshot was inspected
  manually before acceptance; no existing snapshot was overwritten.
- The runtime endpoint pass case proves borrowed and owned fallible
  `EndpointId` construction; generated surface and renamed-facade fixtures
  prove declarations still compile through the hidden static bridge.
- Core endpoint tests compare static, borrowed, and transferred owned
  identities. Public type and rendered Rustdoc owners require the two new
  timeout methods and forbid the retired methods/public static constructor.
- Endpoint planning now proves a configured runtime-endpoint timeout leaves a
  generated endpoint's disabled timeout selection unchanged while
  construction/planning remain inert.
- Macro unit tests prove normalized semantic variants emit equivalent custom
  methods, defaults, retry, authentication placement, and challenge policy.
  A source policy test rejects syntax imports/source enums and
  implementation-crate feature names in emit/generated output.
- Existing runtime endpoint, fixture, UI, and performance call sites now use
  explicit fallible public identity construction.

Commands executed:

```text
cargo test -p concord-macros --lib --locked
cargo fmt --all
cargo fmt --all -- --check
cargo check -p concord-core --lib --all-features --locked
cargo check -p concord-core --all-targets --all-features --locked
cargo check -p concord-macros --all-targets --locked
cargo check -p concord --tests --all-features --locked
cargo check -p concord-xtask --all-targets --locked
cargo run --locked -p concord-xtask -- docs --write
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord --test public_compile --no-default-features --locked -- --test-threads=1
cargo test -p concord --test ui --no-default-features --locked
cargo test -p concord --test ui --all-features --locked
cargo test -p concord-core --test ui --no-default-features --locked
cargo test -p concord-core --test public_types --all-features --locked
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings
cargo run --locked -p concord-xtask -- docs
cargo test -p concord --test rustdoc --all-features --locked
cargo +stable clippy -p concord-core -p concord-extras -p concord -p concord-macros -p concord-xtask --all-targets --all-features --locked -- -D warnings
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord --all-features --locked -- --test-threads=1 --quiet
cargo test -p concord-examples --all-features --locked
cargo +1.85.0 check -p concord-core -p concord-extras -p concord-macros -p concord --all-targets --all-features --locked
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo run --locked -p concord-xtask -- fixtures
git diff --check
git status --short
```

The macro test was run before normalization to establish 65 passing tests and
repeated after the two new owners were added. Several focused checks and
phase-matrix commands were repeated after their documented repairs. Read-only
inspection also used `rg`, `Get-Content`, `git diff`, `git log`, and
`git status` to inventory constructors, bridge call sites, semantic/emit
coupling, feature diagnostics, limits ownership, snapshots, and the complete
patch.

Exact results:

- The pre-change macro owner exited 0 with 65 passed. The first
  post-normalization run had 66 passed and one failed policy test because its
  own literal forbidden string matched the search. Constructing that search
  token removed the false positive; the final owner exited 0 with 67 passed.
- Initial core/facade all-target checks exited 1 after removal of the generic
  conversion because product and test call sites still relied on implicit
  string-to-ID conversion. Internal constructors were tightened to exact
  typed IDs and public call sites changed to fallible construction. Final
  core, macro, façade-test, and xtask checks exited 0.
- The first stable no-default UI run exited 1 only for the deliberate missing
  endpoint-ID snapshot and three line-only mismatches caused by fixture
  reformatting. The new E0277 output was inspected, added with `apply_patch`,
  and the unrelated fixture line positions were restored. Final no-default UI
  exited 0 with 6 harness tests; all-feature UI exited 0 with 8.
- Generated surface exited 0 with 1 passed. Public compile was run serially
  and exited 0 with 7 passed. Core UI exited 0 with all 3 harness owners and
  every exact snapshot. Core public types exited 0 with 25 passed.
- Core lib exited 0 with 360 passed; bounds with 12; lifecycle with 6; security
  with 3. One earlier combined invocation produced output beyond the
  available capture and no result was inferred from it; every command was
  rerun individually to obtain the recorded results.
- Extras exited 0 with 46 unit tests, all integration tests, and its
  compile-fail doctest.
- The complete serialized all-feature façade suite exited 0: all integration,
  7 public-compile, 8 stable exact UI groups, 2 rendered Rustdoc, and doctest
  owners passed.
- Examples exited 0: the construction owner, 7 local-runtime tests, 3 Riot
  contract tests, all targets, and doctests passed.
- Rust 1.85 all-target/all-feature compilation for core, extras, macros, and
  façade exited 0.
- Documentation generation/check exited 0. Warning-denied façade, core, and
  extras Rustdoc exited 0; rendered Rustdoc exited 0 with 2 passed.
- Warning-denied stable Clippy exited 0. Rust 1.97 emitted its localized MSVC
  `linker_messages` warning while linking proc macros; this lint is explicitly
  not promoted by `-D warnings`.
- The fixture driver exited 0, including metadata and the serialized
  no-default public-compile matrix with 7 passed.
- The optimized benchmark no-run command exited 0 and produced
  `concord_overhead-d01aa542161bf3e9.exe`.
- Final format, documentation, Clippy, diff, and status checks are rerun
  immediately before the phase commit.

Unexecuted checks:

- The benchmark executable was not run, so C5 claims no numeric performance
  comparison.
- C7's Rust 1.85 semantic JSON diagnostic harness is not executed early;
  stable exact UI remains the passing owner in C5.
- No graphical editor or manual rust-analyzer acceptance session ran.
- No remote GitHub Actions or independent Linux/macOS platform lane ran.
- No package, registry, legal-metadata, package-name, or publication decision
  was attempted; technical package verification remains C6.
- No external network service was used.

Security/redaction review:

- Failed public ID construction retains only the existing value-free
  `ConfigurationErrorKind::InvalidValue`; the rejected identity is absent
  from `Display`, `Debug`, and sources. The compile-fail input contains no
  protected endpoint.
- Generated static identity construction remains const-validated and does not
  allocate. Public runtime construction validates before allocating borrowed
  text and adopts already-owned `String` input without a duplicate copy.
- The semantic normalization moves/clones Syn nodes with their source spans;
  it does not add runtime data, diagnostics, or secret-bearing model fields.
  Stable parser/UI owners passed unchanged apart from the deliberate new
  endpoint conversion snapshot.
- Timeout renaming changes only immutable configuration naming. Planning,
  lifecycle, authentication recovery, limiting, body work, transport,
  response acquisition, completion, and cancellation ordering remain on the
  existing executor.
- Full core security/lifecycle/bounds, façade lifecycle, exact UI, fixture,
  and example owners passed. Evidence and error output contain no credential,
  rate key, authorization value, protected URL, or native transport data.

Performance impact:

- Generated endpoint IDs retain static storage and zero allocation through
  the hidden const bridge. Ordinary runtime construction now performs the
  required bounded validation and at most one owned allocation.
- Semantic normalization is proc-macro compile-time work over already bounded
  syntax nodes. Timeout renaming and bridge deletion add no runtime work.
- The optimized benchmark target compiles. No measured regression or
  improvement is claimed.

Deviations:
None.

Remaining blockers:

- C6 must remove `dangerous-dev-tools`, eliminate the normal product
  dependency on `concord-test-support`, preserve internal performance probes,
  and prove local package resolution.
- C7 must complete executable locked validation ownership, workflow
  lint/policy checks, passing MSRV semantic diagnostics, automated
  rust-analyzer checks, and documentation/evidence truth repair.
- C8 must run the final local certification matrix and retain external
  editor/platform/legal/registry blockers with exact owners and next actions.
