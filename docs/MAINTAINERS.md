# Maintaining Concord

For people changing Concord itself. Users start at the [README](../README.md).

## Workspace

| Path | Contents |
|---|---|
| `crates/concord` | the public facade: re-exports `concord-core` and, by feature, `concord-extras`, and defines `api!` (a `macro_rules!` that hands `$crate::__macro::concord_core` to the compiler) |
| `crates/concord-core` | the runtime: request composition, native headers, snapshots, the open `Encode`/`Decode`/`CodecError`/`ResponseHead` contracts, `Error`, `Prepared`/`Decoder`; knows no format |
| `crates/concord-extras` | the standard formats (`Json`, `Text`, `Binary`, `Form`, `IgnoreContentType<F>`), written against the public traits only |
| `crates/concord-macros` | the `api!` compiler (proc macro) |
| `examples/quickstart` | the quick start; its tests also run the caller guide, doctest the author guide and the migration guide, and check every quoted excerpt in the documentation |
| `examples/shapes` | three realistic API shapes with wire tests |
| `examples/native` | application-owned native recipes (errors, timeouts, streaming, formats, policies) |
| `examples/multipart` | multipart upload via reqwest's `multipart` feature (enabled only there; tested alone in `verify.sh`) |
| `examples/renamed` | Concord depended on under another name (`$crate` paths) |
| `checks/heap` | peak-heap check that `()` outputs drain without buffering (own crate: global allocator) |
| `checks/clone-compat` | conditional request `Clone` (D08) on the pinned stable and nightly (non-blocking lane) |
| `benches/baselines` | hand-written reqwest wrappers for the `examples/shapes` APIs |
| `benches/criterion` | Criterion runtime suite and allocation counts, Concord vs hand-written reqwest (its `README.md`) |
| `benches/scale` | synthetic-declaration generator and compile-time plan |
| `v4/` | specification (`SEMANTICS.md`), acceptance matrix (`TEST_MATRIX.md`) and records |

## From declaration to request

```text
concord::api! { .. }                                  crates/concord/src/lib.rs
  └─ $crate::__macro::api! { $crate::__macro::concord_core; .. }   the bridge: the runtime's path first
       crates/concord-macros/src/lib.rs               splits the path at `;`, then:
       syntax.rs    parse into a syntax tree           structure only; no meaning
       template.rs  "literal {hole} {owner.hole}"      used by path, subdomain, query/header values
       model.rs     resolve, validate, compose         every rule decided at compile time
       emit.rs      generate Rust                      types, controls, navigation, prepare, Debug, docs
  └─ generated code calls <runtime path>::__private::* crates/concord-core/src/__private.rs (support)
       prepared.rs  Prepared<T>, Decoder<T>            send, send_raw, into_parts, decode, finish
       error.rs     Error, ErrorKind                   static Display, explicit raw accessors
       codec.rs     Encode<T>, Decode<T>, ResponseHead, CodecError   the open format contracts
       value.rs     IntoValue<T>                       the sealed set of accepted conversions
       header.rs    ToHeaderValue (sealed)             how a value placed as a header becomes one
  └─ a statement's format is a type path               resolved by rustc where it is written
       crates/concord-extras/src/                      Json, Text, Binary, Form, IgnoreContentType
```

The macro knows no format: it emits the path you wrote, and rustc resolves it. The runtime files
named in the table below (`__private.rs`, `prepared.rs`, `error.rs`, `codec.rs`) are in
`crates/concord-core/src/`.

The generated code decides at runtime whether optional values are present (`if let`). Which
contributions reach an endpoint's request is fixed by `model.rs`, except for header chains that
name a native `HeaderName` constant: there `__private::HeaderPlan` compares the native names when
the request is built (names and presence only; no value is converted to find its header).

## Where each rule lives

| Rule (SEMANTICS section) | Compile time | Runtime |
|---|---|---|
| Grammar, keywords, statement order (§1) | `syntax.rs`: `Parse for Api`, `parse_child`, `parse_params`, `parse_entries` | — |
| Formats: type paths (resolved by rustc, no codec table), statements, media types (`mime`), removed-form and `via`/alpha hints (§7) | `syntax.rs`: `parse_format`, `parse_output`, `parse_default`, `check_media_type`, `reject_via`; `model.rs`: `alpha` | — |
| Format statements: nearest owner (endpoint first), consumers, statements with nothing to apply to, 8c conflict (§7) | `model.rs`: `nearest`, `endpoint_body`, `endpoint_output`, `flatten`, `check_media_conflict` | — |
| Format codegen: hidden aliases gated by consumers' `cfg`s, body format statics (§7) | `emit.rs`: `format_aliases`, `consumers_cfg`, `format_for`, `prepare_body` | `__private.rs`: `BodyFormat`, `encode_body`, `Parts::finish` (`Content-Type` rule and flag) |
| Accepted attributes (§1) | `syntax.rs`: `docs_only`, `child_attrs`, `parse_params` | — |
| Templates and holes (§1) | `template.rs`: `parse` | `__private.rs`: `format_template` |
| Name resolution, qualification (§2) | `model.rs`: `resolve_ref`, `qualifier` | — |
| Used values, `each` vs scalar, reserved names (§2) | `model.rs`: `Model::build`, `mark`, `flatten` | — |
| Generated-name collisions (§2) | `model.rs`: `check_names` | — |
| Value kinds, controls, initializers (§3) | `emit.rs`: `controls`, `value_inits`, `navigation` | `value.rs`: `IntoValue` |
| Declared base subset (§4) | `model.rs`: `validate_base` (+ the D11 consistency test at the end of the file) | `__private.rs`: `Base::parse`, `Base::from_static` |
| Path and subdomain structure (§4) | `model.rs`: `structural`, `pieces` | `__private.rs`: `Parts::segment`, `Parts::label`, `Parts::host`, `encode_segment` |
| Query/header keys (literal names via `http::HeaderName::from_bytes`, native key paths), reserved headers, literal header values via `http::HeaderValue::from_bytes` (§5) | `model.rs`: `key`, `key_paths`, `RESERVED_HEADERS`, `rejected_value`, `resolve_owner`; `emit.rs`: `header_statics` (literal names and ASCII literal values as statics, one checked accessor per key path) | `__private.rs`: `Parts::query`, `Parts::header`, `header_literal`, `NativeKey`, `reserved_name`, `Parts::finish` |
| Native header identity: three-valued relation, compile-time errors when provable, run-time plan (§5) | `model.rs`: `relation`, `compose_native`, `set_before`, `header_seq`; `emit.rs`: `native_headers_code` | `__private.rs`: `HeaderPlan`, `HeaderEntry`, `HeaderOp` |
| Header value routes: literal / formatted / `ToHeaderValue` (sealed), text follows `http` (§5) | `model.rs`: `Value::text`; `emit.rs`: `source_code`, `debug_fields` | `header.rs`: `ToHeaderValue`; `__private.rs`: `Parts::header`, `Text`, `text` |
| Composition: slots, `append`/`override`/`else inherit`/`remove` (§5) | `model.rs`: `resolve_owner`, `compose`, `compose_family` | generated `prepare` (`emit.rs`: `prepare_body`) |
| Build-check order, failing `Display` (§6) | `emit.rs`: `prepare_body` | `__private.rs`: `display_into`, `encode_segment`, `Parts` |
| Execution, decoding (§6, §7) | `emit.rs`: endpoint `send`/`prepare` | `prepared.rs`, `__private.rs`: `decoder`, `drain`, `encode_body`, `Parts::finish` (the `Accept` suggestion); `codec.rs` |
| Errors (§8) | — | `error.rs` |
| Sensitivity (§9) | `model.rs`: `SENSITIVE_NAMES`, `sensitive_name`, `refresh_sensitivity`, `propagate_sensitivity`, `native_conditions` (run-time conditions for native keys); `emit.rs`: `cond_expr`, `flag_expr` | `__private.rs`: `SENSITIVE_NAMES` (kept equal to the macro's by a unit test), `sensitive_name`, `Withheld`, `redacted_url`, `DebugDisplay`, `DebugHeader`, `Redacted`, `Parts::header` (flag OR), `HeaderPlan` (flags); `prepared.rs`: `Debug` |
| Generated documentation | `emit.rs`: `owner_docs`, `defaults_line`, `request_summary`, `controls`, `argument_docs`, `override_notes`, `initially` | — |

## Changing a semantic rule

1. Change the rule in `v4/SEMANTICS.md` first; it is the single current statement of meaning.
2. Implement it in the places above, compile-time first.
3. Add or update an executable case: a runtime test in `crates/concord/tests/` and/or a UI test in
   `crates/concord/tests/ui/`; list it in `v4/TEST_MATRIX.md`.
4. Update the guides that teach it (`docs/AUTHOR_GUIDE.md`, `docs/CALLER_GUIDE.md`,
   `docs/CUSTOM_FORMATS.md` for the format contracts) and the crate documentation. Their code is
   compiled or checked against its source, so a stale example fails the build.
5. A breaking change gets an old/new entry in `docs/MIGRATION.md` and, for a design decision, a
   record in `v4/finalization/DECISIONS.md`.
6. Run `scripts/verify.sh` (below).

## Running the checks

```sh
scripts/verify.sh                   # every gate that runs on all platforms
scripts/verify.sh --package --bench # also the packaged-crate consumers and benchmark compilation
scripts/verify.sh --nightly         # also the non-blocking nightly `Clone` checks
```

It prints one `PASS`/`FAIL` line per step (logs in `target/verify-*.log`) and fails if a tracked
file changed while it ran (for example `Cargo.lock`). Every cargo command uses `--locked`.

`scripts/verify.sh` is the only list of gates: `.github/workflows/ci.yml` calls it (Linux and
Windows both with `--package --bench`, a nightly lane with `--nightly-only`), and its
`ci-inventory` step fails if the workflow names a cargo command of its own or selects gates by
platform. Every step runs `python`, `cargo` and `tar` behind `scripts/argv-guard.sh`, which
measures the command line as Windows would quote it (`scripts/argv-length.py`, in UTF-16 units)
and refuses one over the Windows limit (32,767) on every platform, so a script that passes bulk
data as an argument fails locally too. The `consumers` steps need the
network once for `checks/consumers/facade-gzip`, which keeps its own `Cargo.lock` and fetches
reqwest's compression dependencies (`cargo fetch --locked`); the other consumers build offline. The minimum supported
Rust (`rust-version`) and the toolchain pinned in `rust-toolchain.toml` are the same release, so
the ordinary run is also the minimum-version check; its `toolchain` step fails if they diverge,
if a production package does not inherit the minimum, or if a script selects another stable
release. Which platforms ran for a revision is recorded with that revision's campaign.

Documentation checks run inside `cargo test --workspace`:

- the doctests of `crates/concord` (crate documentation and doc comments), which must pass with
  default features and with `--no-default-features`;
- `examples/quickstart`: the author and migration guides as doctests, the caller guide as tests,
  and `tests/doc_excerpts.rs`, which fails when a quoted excerpt in `README.md` or `docs/*.md`
  no longer matches its source, or when a Rust block there is neither compiled nor a labelled
  excerpt. A document outside its `DOCTESTED` list (the format guide, for one) may hold Rust
  blocks only as labelled excerpts of real files.

## UI snapshots

`crates/concord/tests/ui/*.rs` are compile-fail cases run by `trybuild`; the expected compiler
output is the neighbouring `.stderr` file. trybuild builds them with the features of the test
run, so `ui/` holds only feature-independent cases (it runs in every feature lane); cases whose
output depends on a feature live in `ui_json/` (run where `json` is on) and `ui_features/` (run
where neither `json` nor `form` is on). Bless each directory in a lane that runs it, and only that
directory:

```sh
TRYBUILD=overwrite cargo test -p concord --test ui --locked                          # ui/, ui_json/
TRYBUILD=overwrite cargo test -p concord --test ui --locked --no-default-features    # ui/, ui_features/
git diff crates/concord/tests/
```

Review every changed snapshot before committing: a new message is a user-visible change
([`docs/DIAGNOSTICS.md`](DIAGNOSTICS.md) quotes several of them, and its excerpt check fails until
the guide is updated). Snapshots are pinned to the toolchain in `rust-toolchain.toml` (1.99.1, also
the minimum supported Rust); rustc-origin messages change between compiler versions, so raising the
pin means reviewing every changed snapshot.

## Benchmarks and scale

```sh
cargo bench -p concord-bench --bench runtime  # Criterion: construction, headers, composition, bodies, decoding, loopback
cargo run --release -p concord-bench --bin alloc-count   # allocation counts per case
cargo test -p concord-heap-check --locked    # `()` outputs drain without buffering
```

Compile cost and generated size use synthetic declarations:

```sh
python3 benches/scale/gen.py suite /tmp/concord-scale    # a cargo workspace of generated crates
benches/scale/timing_plan.sh /tmp/concord-scale          # prints the hyperfine commands (dry run)
benches/scale/timing_plan.sh /tmp/concord-scale run      # runs them; JSON in /tmp/concord-scale/timings/
```

`gen.py` is deterministic; `gen.py --help` and its module documentation describe the shapes
(`rest`, `deep`, `shared`, `override`, `optionals`, `refs`, `fanout`, `typed`) and consumers.
`--syntax via` or `--syntax alpha` generates the same workloads for an older Concord, for A/B
suites. Timing needs an
otherwise idle machine; record the toolchain and commit with the results. Earlier measurements are
in `v4/evidence/PERF.md` (alpha) and `v4/refinement/evidence/tracks/compiler.md`, both taken with
rustc 1.97.1 (the second also with a nightly for expansion), before the 1.99.1 pin: compare new
results with them only on the same compiler, and pass `TOOLCHAIN=` to `timing_plan.sh`
explicitly so the plan names the compiler you measure.

## The generated-code bridge

Generated code calls `<runtime path>::__private::*`, where the runtime path is
`$crate::__macro::concord_core` (the facade's `__macro` module re-exports `concord_core` and the
proc macro): items that are public only so that expansions in other crates can reach them. They are
`#[doc(hidden)]`, not part of the user API, and may change in any release. Consequences:

- `concord`, `concord-core` and `concord-macros` change together. `concord` depends on all three
  and on `concord-extras`, and `concord-extras` on `concord-core`, with exact version
  requirements (`version = "=4.1.0"`); every crate takes the workspace version, so a
  published set can never mix. `concord-extras` uses only the public traits of `concord-core`.
- Any change to a `__private` signature that `emit.rs` relies on is made in the same commit as the
  `emit.rs` change, and the crates' versions move together.
- Users never name `__private` items; a UI or doctest that needs one is a sign that a public API
  is missing.
- The runtime path reaches the proc macro explicitly (as the first tokens, ended by `;`), so
  renamed dependencies work (`examples/renamed`). A format is not part of the bridge: its path is
  the one the user wrote.

## Records and evidence

- `v4/modular-codecs/` — the current increment (four crates, formats as type paths,
  metadata-aware codecs, `validate_header_names()`): `STATUS.md`, `DECISIONS.md` (MC*),
  `TEST_MATRIX.md`, `REVIEW_TRIAGE.md`, `MODEL_RUNS.md`, `PERF.md`, `FINAL.md`, frozen
  `reports/`.
- `v4/refinement/` — an earlier refinement campaign: `DECISIONS.md` (D01–D18, what changed and
  why), `STATUS.md` (gates, platforms, process events), `evidence/` (baseline receipts and
  specialist tracks), `reviews/` (independent design reviews).
- `v4/SEMANTICS.md` and `v4/TEST_MATRIX.md` — the current contract and its executable cases.
- Other files in `v4/` (`DECISIONS.md`, `PRODUCT.md`, `EXAMPLES.md`, `STATUS.md`, `evidence/`)
  record the V4 alpha and are historical; some use alpha syntax.
- V3 is preserved at tag `concord-v3-preserved`.
