# Maintaining Concord

For people changing Concord itself. Users start at the [README](../README.md).

## Workspace

| Path | Contents |
|---|---|
| `crates/concord` | the runtime and the `api!` entry point (`macro_rules!` forwarding to the compiler) |
| `crates/concord-macros` | the `api!` compiler (proc macro) |
| `examples/quickstart` | the quick start; its tests also run the caller guide, doctest the author guide and the migration guide, and check every quoted excerpt in the documentation |
| `examples/shapes` | three realistic API shapes with wire tests |
| `examples/native` | application-owned native recipes (errors, timeouts, streaming, formats, policies) |
| `examples/multipart` | multipart upload via reqwest's `multipart` feature (enabled only there; tested alone in `verify.sh`) |
| `examples/renamed` | Concord depended on under another name (`$crate` paths) |
| `checks/heap` | peak-heap check that `()` outputs drain without buffering (own crate: global allocator) |
| `checks/clone-compat` | conditional request `Clone` (D08) on 1.85, current stable and nightly (non-blocking lane) |
| `benches/baselines` | hand-written reqwest wrappers for the `examples/shapes` APIs |
| `benches/criterion` | Criterion runtime suite and allocation counts, Concord vs hand-written reqwest (its `README.md`) |
| `benches/scale` | synthetic-declaration generator and compile-time plan |
| `v4/` | specification (`SEMANTICS.md`), acceptance matrix (`TEST_MATRIX.md`) and records |

## From declaration to request

```text
concord::api! { .. }                                  crates/concord/src/lib.rs
  └─ __private::api_impl! { $crate; .. }              the proc macro receives the crate path first
       crates/concord-macros/src/lib.rs               splits `$crate;`, then:
       syntax.rs    parse into a syntax tree           structure only; no meaning
       template.rs  "literal {hole} {owner.hole}"      used by path, subdomain, query/header values
       model.rs     resolve, validate, compose         every rule decided at compile time
       emit.rs      generate Rust                      types, controls, navigation, prepare, Debug, docs
  └─ generated code calls $crate::__private::*         crates/concord/src/__private.rs (runtime support)
       prepared.rs  Prepared<T>, Decoder<T>            send, send_raw, into_parts, decode, finish
       error.rs     Error, ErrorKind                   static Display, explicit raw accessors
       repr.rs      Encode<T>, Decode<T>, Json/Text/Binary, BodyError
       value.rs     IntoValue<T>                       the sealed set of accepted conversions
       header.rs    ToHeaderValue                      how a value placed as a header becomes one
```

The generated code decides at runtime only whether optional values are present (`if let`); which
contributions reach an endpoint's request is fixed by `model.rs`.

## Where each rule lives

| Rule (SEMANTICS section) | Compile time | Runtime |
|---|---|---|
| Grammar, keywords, `via` required, statement order (§1) | `syntax.rs`: `Parse for Api`, `parse_child`, `parse_params`, `parse_payload`, `parse_entries` | — |
| Accepted attributes (§1) | `syntax.rs`: `docs_only`, `child_attrs`, `parse_params` | — |
| Templates and holes (§1) | `template.rs`: `parse` | `__private.rs`: `format_template` |
| Name resolution, qualification (§2) | `model.rs`: `resolve_ref`, `qualifier` | — |
| Used values, `each` vs scalar, reserved names (§2) | `model.rs`: `Model::build`, `mark`, `flatten` | — |
| Generated-name collisions (§2) | `model.rs`: `check_names` | — |
| Value kinds, controls, initializers (§3) | `emit.rs`: `controls`, `value_inits`, `navigation` | `value.rs`: `IntoValue` |
| Declared base subset (§4) | `model.rs`: `validate_base` (+ the D11 consistency test at the end of the file) | `__private.rs`: `Base::parse`, `Base::from_static` |
| Path and subdomain structure (§4) | `model.rs`: `structural`, `pieces` | `__private.rs`: `Parts::segment`, `Parts::label`, `Parts::host`, `encode_segment` |
| Query/header keys, reserved headers, literal header values (§5) | `model.rs`: `key`, `RESERVED_HEADERS`, `resolve_owner`; `emit.rs`: `header_statics` (names and literal values as statics) | `__private.rs`: `Parts::query`, `Parts::header`, `Parts::finish` |
| Header value routes: literal / formatted / `ToHeaderValue`, text policy (§5) | `model.rs`: `Value::text`; `emit.rs`: `source_code`, `debug_fields` | `header.rs`: `ToHeaderValue`; `__private.rs`: `Parts::header`, `Text`, `text` |
| Composition: slots, `append`/`override`/`else inherit`/`remove` (§5) | `model.rs`: `resolve_owner`, `compose`, `compose_family` | generated `prepare` (`emit.rs`: `prepare_body`) |
| Build-check order, failing `Display` (§6) | `emit.rs`: `prepare_body` | `__private.rs`: `display_into`, `encode_segment`, `Parts` |
| Execution, decoding (§6, §7) | `emit.rs`: endpoint `send`/`prepare` | `prepared.rs`, `__private.rs`: `decoder`, `drain`, `encode_body`; `repr.rs` |
| Errors (§8) | — | `error.rs` |
| Sensitivity (§9) | `model.rs`: `SENSITIVE_NAMES`, `sensitive_name`, `refresh_sensitivity`, `propagate_sensitivity` | `__private.rs`: `redacted_url`, `DebugDisplay`, `DebugHeader`, `Redacted`, `Parts::header` (flag OR); `prepared.rs`: `Debug` |
| Generated documentation | `emit.rs`: `owner_docs`, `controls`, `argument_docs`, `override_notes`, `initially` | — |

## Changing a semantic rule

1. Change the rule in `v4/SEMANTICS.md` first; it is the single current statement of meaning.
2. Implement it in the places above, compile-time first.
3. Add or update an executable case: a runtime test in `crates/concord/tests/` and/or a UI test in
   `crates/concord/tests/ui/`; list it in `v4/TEST_MATRIX.md`.
4. Update the guides that teach it (`docs/AUTHOR_GUIDE.md`, `docs/CALLER_GUIDE.md`) and the crate
   documentation. Their code is compiled, so a stale example fails the build.
5. A breaking change gets an old/new entry in `docs/MIGRATION.md` and, for a design decision, a
   record in `v4/refinement/DECISIONS.md`.
6. Run `scripts/verify.sh` (below).

## Running the checks

```sh
scripts/verify.sh          # fmt, clippy, workspace tests, feature combinations, rustdoc -D warnings
scripts/verify.sh --msrv   # also the runtime suites and examples on Rust 1.85
```

It prints one `PASS`/`FAIL` line per step (logs in `target/verify-*.log`) and fails if a tracked
file changed while it ran (for example `Cargo.lock`). Every cargo command uses `--locked`. The
steps match `.github/workflows/ci.yml`; CI declares Linux and Windows jobs, but this revision was
verified on Linux x86_64 only.

Documentation checks run inside `cargo test --workspace`:

- the doctests of `crates/concord` (crate documentation and doc comments), which must pass with
  default features and with `--no-default-features`;
- `examples/quickstart`: the author and migration guides as doctests, the caller guide as tests,
  and `tests/doc_excerpts.rs`, which fails when a quoted excerpt in `README.md` or `docs/*.md`
  no longer matches its source, or when a Rust block there is neither compiled nor a labelled
  excerpt.

## UI snapshots

`crates/concord/tests/ui/*.rs` are compile-fail cases run by `trybuild`; the expected compiler
output is the neighbouring `.stderr` file.

```sh
TRYBUILD=overwrite cargo test -p concord --test ui --locked
git diff crates/concord/tests/ui/
```

Review every changed snapshot before committing: a new message is a user-visible change
([`docs/DIAGNOSTICS.md`](DIAGNOSTICS.md) quotes several of them, and its excerpt check fails until
the guide is updated). Snapshots are pinned to the toolchain in `rust-toolchain.toml` (1.97.1);
rustc-origin messages change between compiler versions, which is why the MSRV job does not run
the UI test.

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
(`rest`, `deep`, `shared`, `override`, `optionals`, `refs`) and consumers. Timing needs an
otherwise idle machine; record the toolchain and commit with the results. Earlier measurements are
in `v4/evidence/PERF.md` (alpha) and `v4/refinement/evidence/tracks/compiler.md`.

## The generated-code bridge

Generated code calls `$crate::__private::*`: items that are public only so that expansions in
other crates can reach them. They are `#[doc(hidden)]`, not part of the user API, and may change
in any release. Consequences:

- `concord` and `concord-macros` change together. `concord` depends on `concord-macros` with an
  exact version requirement (`version = "=4.0.0-alpha.1"`), and both crates take the workspace
  version, so a published pair can never mix.
- Any change to a `__private` signature that `emit.rs` relies on is made in the same commit as the
  `emit.rs` change, and both crates' versions move together.
- Users never name `__private` items; a UI or doctest that needs one is a sign that a public API
  is missing.
- The crate path reaches the proc macro explicitly (`$crate;` as the first tokens), so renamed
  dependencies work (`examples/renamed`).

## Records and evidence

- `v4/refinement/` — the current refinement campaign: `DECISIONS.md` (D01–D18, what changed and
  why), `STATUS.md` (gates, platforms, process events), `evidence/` (baseline receipts and
  specialist tracks), `reviews/` (independent design reviews).
- `v4/SEMANTICS.md` and `v4/TEST_MATRIX.md` — the current contract and its executable cases.
- Other files in `v4/` (`DECISIONS.md`, `PRODUCT.md`, `EXAMPLES.md`, `STATUS.md`, `evidence/`)
  record the V4 alpha and are historical; some use alpha syntax.
- V3 is preserved at tag `concord-v3-preserved`.
