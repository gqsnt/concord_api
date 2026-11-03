# Track D — Compiler maintainer (resolved model, codegen, hygiene, algorithmic growth)

## 0. Identity, settings, scope

- **Source.** SNAP `/home/canarit/projects/concord_v4_refine_work/snapshots/compiler-8f7adc2`,
  `.concord-review.json` → `original_commit 8f7adc2019f3ee31af087c69286d559bca1c173a`, tree
  `26e6a14b…`; snapshot HEAD `977e828` (branch `master` = control, left checked out and clean).
- **Toolchains.** Control builds/tests: rustc **1.97.1** (SNAP `rust-toolchain.toml`). Expansion
  (`-Zunpretty=expanded`), `-Ztime-passes` and rustdoc JSON: `rustc 1.101.0-nightly (e9bbadf68
  2026-10-12)`. MSRV `1.85` used only in one robustness probe. Linux 7.0.0-42, x86_64.
- **Model/effort.** Claude Opus 5.5 (`claude-opus-5-5`, from the session's system prompt). The
  assignment requested `xhigh`; **effort not observable from inside the session**.
- **Interruption.** The host agent process restarted at ~00:43; no partial output was reused
  without re-checking (state verified, every measurement below was re-run or completed after).
- **Write set.** SNAP (prototype branches only, `master` untouched), EXP
  `/home/canarit/projects/concord_v4_refine_work/exp/compiler`, this report. No timing benchmark
  was run; one wall-time column is labelled *noisy, concurrent builds*.

## 1. Summary of recommendations

| ID | Recommendation | Smallest implementation (SNAP branch / EXP patch) | Evidence kind |
|---|---|---|---|
| D16 hidden emitter state | **CHANGE** (low priority): explicit emission context | `d16-emit-context` (+54/−54, `emit.rs` only); alt. `d16-krate-param` (+35/−41) | byte-identical expansion of 10 targets (21,028 lines), 101 tests, clippy, fmt |
| D16 positional docs dropped | **CHANGE** (defect) | `docs-positional` (+36/−5) | rustdoc JSON before/after |
| D16 duplicated rules | **KEEP** stage split (no third crate); **CHANGE** header-name length (defect); delete dead sensitivity pre-computation + double composition later (DEFER, maintenance) | `defect-header-name-len` (+8) | downstream repro + fix |
| D17 specialization growth | **KEEP** D1 | — | expansion sizes, 6 dimensions |
| D17 generator/consumers | **CHANGE**: adopt generator v2 | `d17-scale-generator` (`benches/scale/gen.py`, +384/−21) | determinism + legacy byte-identity + build/run of every member |
| C16 long declarations | **CHANGE** Debug statement form (hardening); **DEFER**/document depth and override-nesting limits | `c16-debug-statements` (+13/−6) | crash repro on nightly, fixed; thresholds measured |
| Inline (3ba81c5) | **KEEP** blanket `#[inline]` until the fanout measurement below is run | — | expansion + symbol evidence only |
| Hygiene/naming | **KEEP** (raw idents, forwarding, rename, `no_implicit_prelude`, strict lints all pass); flat-name collisions → document, DEFER renaming to DSL/API track | — | 10 downstream crates |

## 2. Commands actually run (everything else in this report is labelled reasoning)

```text
# control tests / prototypes (in SNAP, CARGO_TARGET_DIR=SNAP/target, CARGO_BUILD_JOBS=6, offline)
cargo test --locked --workspace            # each prototype branch: 101 passed, 0 failed
cargo clippy --locked -q --workspace --all-targets -- -D warnings ; cargo fmt --all -- --check
EXP/d16/expand_all.sh SNAP EXP/d16/expanded-{control,variantA,variantB}; diff -r ...   # identical
# D17
python3 -I SNAP/benches/scale/gen.py OUT N CONCORD   (legacy form, old vs new: byte-identical, N=50/200/1000)
python3 -I SNAP/benches/scale/gen.py suite EXP/d17/suite   (twice → identical suite.json)
cargo build --offline --locked --workspace   (suite, dev) ; ./target/debug/{use5,use100,use1000,fan_app}
cargo +1.97.1 build --offline --locked --release -p scale50 -p use5 -p use100 -p fan_app ; nm -C -S
python3 -I EXP/d17/sizes.py SNAP ; python3 -I EXP/d17/fnsizes.py EXP/d17/sizes/*.rs
EXP/d17/macro_time.sh SNAP    (noisy) ; EXP/d17/limits.sh SNAP
# hygiene
EXP/hygiene/run.sh            (1.97.1; rustdoc JSON with nightly)
```

## 3. D16 — Compiler structure

### 3.1 One endpoint traced (`examples/shapes/src/s2_scoped.rs`: `CatalogApi.regional.org.products`)

Declaration: `GET products(version?: String) -> Json<Vec<Product>> { path "products"; headers {
override "x-api-version" = version; } }` under `scope org(org) { path "orgs/{org}/catalog"; }`
under `scope regional(region) { subdomain "{region}"; }` under a client with `#[sensitive]
header("x-api-key") api_key`, `header("x-api-version") api_version = DEFAULT_API_VERSION` and
`headers { "accept" = "application/json"; }`.

| Phase | What happens | Code (control) |
|---|---|---|
| Entry | `concord::api!` forwards `$crate; tokens` to `__private::api_impl`; the proc macro splits the crate path at the first top-level `;` | `crates/concord/src/lib.rs:189-194`, `concord-macros/src/lib.rs:13-33` |
| Syntax | `Api::parse` → `parse_owner_body` → `parse_child` (method, `parse_params`/`parse_placement`) → `Stmts{path, headers: [Entry{Override, "x-api-version", Rhs::Ref(version)}]}`; no meaning assigned | `syntax.rs:128-162, 181-222, 258-293, 339-412, 414-503, 514-559` |
| Flatten | owner index, type name `CatalogApiRegionalOrgProducts` (concatenated PascalCase), dotted name, state name, values (`debug_sensitive := sensitive`) | `model.rs:1277-1411` |
| Value checks | duplicate values, `__` prefix, `http` on client | `model.rs:254-292` |
| Resolution | path → `template::parse`/`split('/')` → literal pchar checks → `structural` → `pieces` → `resolve_ref` + `mark` (liveness); header entry → `key()` → `resolve_ref` → `Contrib{op, key, source, total}` | `model.rs:496-732`, `template.rs:13-124` |
| Sensitivity | compose **every** owner with errors discarded to collect slot histories → worklist → `refresh_sensitivity` | `model.rs:300-306, 823-882, 776-821` |
| Composition | `compose(index)` → root→endpoint chain; labels deepest-first, segments root-first; `compose_family` builds `Slot{key:"x-api-version", items:[Override{src: version, fallback:[Emit(client placement)]}]}` | `model.rs:308-314, 901-1040` |
| Names | liveness, `check_names` per generated type | `model.rs:316-331, 1042-1147` |
| Emit (type) | struct `{__parent, version}`, `set_version`/`maybe_version`, `Debug` | `emit.rs:220-397` |
| Emit (entry) | `CatalogApiRegionalOrg::products(&self)` | `emit.rs:418-478` |
| Emit (prepare) | `static __META`, bindings `__o2/__o1/__o0` up the chain, one block per label/segment, one statement or nested `if let` per slot | `emit.rs:675-788, 624-673, 606-622, 532-604` |
| Runtime | `Parts::new` → `label` → `host` → `segment_static`/`segment` → `header`×3 → `finish` → `Prepared::new`; `send` → `Prepared::send` → `Client::execute` → `Decoder::decode` | `__private.rs:163-341`, `prepared.rs:47-54, 123-143`, `error.rs:49-81` |

Generated `prepare` (from `EXP/d16/expanded-control/shapes.rs`, abbreviated):
```rust
let __o2 = &*self.__parent.__s; let __o1 = &*__o2.__parent.__s; let __o0 = &*__o1.__parent.__s;
let mut __p = Parts::new(&__META, &__o0.__base);
{ let mut __c = String::new(); display_into(&mut __c, &__o1.region); __p.label(__c, "subdomain label `{region}`")?; }
__p.host()?; __p.segment_static("orgs"); { ...encode_segment(&mut __c, &__o2.org)... } __p.segment_static("catalog"); __p.segment_static("products");
__p.header("x-api-key", &__o0.api_key, true)?;
if let Some(__v) = &self.version { __p.header("x-api-version", &*__v, false)?; } else { __p.header("x-api-version", &__o0.api_version, false)?; }
__p.header("accept", "application/json", false)?;
__p.finish::<Json<Vec<Product>>>(Method::GET, &__o0.__http, __body)
```
A maintainer can follow this without learning an orchestration system: the phases are distinct
and the resolved model (`Composed`, `Slot`, `Item`) drives both behaviour and `__META`.

### 3.2 Hidden state: the thread-local crate path

`emit.rs:14-34`: `emit()` stores the `$crate` tokens in `thread_local! KRATE` (default
`::concord`); nine helpers read it through `krate()`. Reasoning: no observable bug — every
expansion overwrites it before use and emission is single-threaded, so concurrent rust-analyzer
expansions cannot cross-contaminate. The costs are maintainability: helper-level code silently
falls back to `::concord` when not called through `emit()` (wrong under a renamed dependency),
and the data dependency is invisible in signatures.

**Prototypes (executable, in SNAP):**

| Variant | Branch / patch | Diff | Output |
|---|---|---|---|
| A: thread `k: &TokenStream` through the 9 helpers | `d16-krate-param` / `EXP/patches/d16-krate-param/` | `emit.rs` +35/−41 (74 changed lines) | byte-identical |
| B: `struct Cx { model, k, vis }` passed as first argument; removes the separate `model`/`vis` params and the dead `owner` param of `component_code` | `d16-emit-context` / `EXP/patches/d16-emit-context/` | `emit.rs` +54/−54 (107 changed lines) | byte-identical |

Evidence: `EXP/d16/expand_all.sh` expands the 8 `concord` integration-test targets plus
`examples/shapes` and `examples/renamed` (21,028 formatted lines); `diff -r` against the control
is empty for both variants; `cargo test --workspace` 101 passed / 0 failed (UI snapshots
unchanged); clippy `-D warnings` and fmt clean.

**Does it genuinely simplify?** Modestly. It deletes hidden global state and a silent default and
makes the crate-path dependency explicit and unit-testable; net line count is unchanged and the
real complexity (`prepare_body`/`source_code`) is untouched. Recommendation **CHANGE, low
priority, B** (one parameter convention for all emission functions; future emission settings go
into `Cx` without signature churn). A is acceptable if the smallest diff is preferred.
Maintainer note found while prototyping (see §6): `Span::call_site()` of `api_impl!` lies inside
`concord::api!`'s `macro_rules` expansion, so generated locals must never be written inside
`quote_spanned!(user_span => …)` — they become invisible (E0425 "not accessible due to macro
hygiene"). The control is correct; this deserves a comment in `emit.rs`.

### 3.3 Rules owned twice (macro vs runtime; model vs emitted metadata)

| Rule | Owners | Assessment |
|---|---|---|
| Header value = visible ASCII/SP/HT | macro literal check `model.rs:686-699`; runtime formatted values `__private.rs:240-250`; content type `__private.rs:128-131` | Stage split is necessary (runtime values); the same predicate is written 3×. Literal header values are **re-validated and re-allocated at every `prepare`** (`emit.rs:587-591` passes the literal to `Parts::header`). Keep; add cross-reference comments; dropping the runtime re-check for literals is a perf micro-change to measure first. A shared crate is not justified (a proc-macro crate cannot export functions to the runtime). |
| Header name token / length | macro `key()` `model.rs:741-772`; runtime precondition of `HeaderName::from_static` (`__private.rs:254`), which **panics** | **DEFECT** (§8.1): the macro does not enforce `from_static`'s 65,535-byte limit. |
| DNS label | macro literal labels `model.rs:573-598`; runtime `Parts::label` `__private.rs:177-196` | Literal labels are re-validated at runtime every request (`emit.rs:636-638`, `String::from(lit)`), while literal path segments are not (`segment_static`, `emit.rs:639`). Inconsistent but harmless. |
| Base URL | macro `validate_base` `model.rs:1159-1275`; runtime `Base::parse` `__private.rs:46-70`; `Base::from_static` re-parses on **every** `Client::new` and panics if the two disagree (`emit.rs:272`, `__private.rs:73-78`) | Deliberate (macro accepts a strict subset). Prose copy of the rule in `with_base_url`'s generated doc (`emit.rs:278-280`) omits the port-0 rejection enforced at `__private.rs:55`. |
| Built-in method names | collision table `model.rs:1083-1104`; emitted names `emit.rs:267, 281, 379, 384` | Two hand-synchronised lists; a shared `const` in the macro crate would make it one. |
| Effective sensitivity | (1) initial per-contribution/component sensitivity `model.rs:486, 642, 675, 709` — **dead**: always overwritten by `refresh_sensitivity` (`776-821`) before it is read; (2) slot sensitivity computed by the worklist (`847-881`, result vector discarded) and again by `compose_family` (`984, 994, 1004`) | Same rule (name ∨ any history contribution reads a withheld value) evaluated by two code paths; they agree only because both consume the same `history`. |
| Sensitivity carried to runtime | three carriers: index/key lists in `EndpointMeta` (`emit.rs:773-781`), a per-call `bool` for headers (`emit.rs:600-601` → `HeaderValue::set_sensitive`, read by `prepared.rs:100`), `Redacted` in generated `Debug` (`emit.rs:197-201`) | `redacted_url` (`__private.rs:345-406`) **re-derives** which URL pieces were placements by re-splitting the final URL (labels from the left, segments from the right, query keys by decoding). Correct only under invariants enforced elsewhere (values percent-encoded, labels validated, one trailing slash stripped from the base). Reasoning: no counterexample found; KEEP (lazy, Debug-only cost), but this is the one place where semantics are inferred from generated strings. |
| Composition | `compose` runs twice per owner (`model.rs:827-829` with errors discarded at `826`, and `309-314`); for scopes/client the second run exists only to report errors | Merge into one pass when touching this code (DEFER; no behaviour change). |

### 3.4 Generated-doc facts not derived from the resolved model

Evidence: rustdoc JSON of `EXP/hygiene/docs` (`EXP/hygiene/out/rustdoc_items.txt`, control) and
the same crate on `docs-positional` (`EXP/hygiene/out/rustdoc_items_docs_positional.txt`).

1. **`///` on required values and required bodies is accepted and silently dropped** — they have
   no setter (`emit.rs:117-187` is the only consumer of `Value::docs`) and entry methods do not
   mention them. The project's own `examples/shapes` loses "The API key; sent on every request,
   never shown by Concord." from `CatalogApi::new`. (**DEFECT**, §8.2.)
2. Scope navigation methods append " Enters scope `…`." with no blank line (`emit.rs:433-434`),
   so it merges into the user's last paragraph ("Each region has its own host.\nEnters scope
   `Catalog.regional`."); endpoint navigation inserts a separator (`emit.rs:455-459`).
3. `with_base_url` doc is hand-written prose duplicating (incompletely) the runtime rule.
4. The same "`GET` endpoint `X.y`" / "Scope `X.y`" lines are formatted in two places
   (`emit.rs:410-411` and `430, 454`). Facts available in the model but not surfaced (placement
   key, sensitivity, initializer) are a design option, not a defect.

Prototype `docs-positional` (+36/−5): a "# Arguments" list derived from the model's positional
values/body (declaration order, body position honoured) on `new` and navigation methods, plus the
missing separator. Tests 101/0, `cargo doc -D warnings` on shapes clean.

### 3.5 Other misplaced or dead responsibility

- `plain()` duplicated: `model.rs:227-230`, `emit.rs:40-43`.
- `component_code`'s `owner` parameter is unused (`emit.rs:627, 642`); `Owner::state_name` is
  computed for endpoints that have no state struct (`model.rs:1361`).
- `encode_body` has a duplicated doc line (`__private.rs:119-121`).
- `Places::owner_var` finds an owner by linear search of the chain per value reference
  (`emit.rs:486-498`).
- `Model::build` stops at phase boundaries (`model.rs:251, 292, 298`): a duplicate value hides
  template/reference errors in the same declaration. Reasonable gating; noted for the UI suite
  ("separate independent failures").

## 4. D17 — Code-generation growth

### 4.1 Generated size, one dimension at a time (measured, non-timing)

`EXP/d17/sizes.py` (expansion with nightly `-Zunpretty=expanded`, rustfmt, non-blank lines;
tokens by regex lexer). Raw: `EXP/d17/sizes/results.json`, expansions `EXP/d17/sizes/*.rs`.

| Dimension (fixed elsewhere) | Points → expanded lines | Growth |
|---|---|---|
| Endpoints N (`rest`) | 10 → 1,558; 50 → 7,494; 200 → 29,754; 1,000 → 148,474 (1.23 M tokens) | linear, 148–156 lines/endpoint |
| Scope depth D, 10 endpoints at the leaf | 1 → 1,148; 4 → 1,670; 16 → 3,794; 64 → 12,597 | linear, ≈ 183 lines/level (≈ 10.6 per leaf endpoint per level + scope overhead) |
| Depth D, 2 endpoints at **every** level | 4 → 1,290; 16 → 6,876; 64 → 59,065 | **quadratic** ≈ 10.3·D² + 265·D (Σ depth; inherent to specialization) |
| Inherited contributions I per endpoint (50 endpoints) | 0 → 7,494; 14 → 8,858; 56 → 12,950; 224 → 29,318 | linear in E×I, 1.95 lines (16.6 tokens) per endpoint per contribution |
| Optional override chain K levels (10 endpoints) | 1 → 1,026; 4 → 1,386; 16 → 2,886; 64 → 9,020 | linear, ≈ 128 lines/level; nesting depth K+1 |
| Optional values M per endpoint (10 endpoints) | 1 → 868; 8 → 2,548; 32 → 8,308; 128 → 31,348 | linear, 24 lines per value per endpoint |

Per function (`EXP/d17/fnsizes.txt`): `prepare` is 39 lines (`rest`), 455 (224 inherited
contributions), up to 668 (depth 64), 400 (128 optionals), 342 (65-level override chain);
`Debug::fmt` up to 516 lines (128 values); everything else ≤ 14 lines except `new` with many
initialized values (393 lines, generic).

### 4.2 Algorithmic complexity (source reasoning + the measurements above)

Notation: O = owners, D = depth, C = contributions on a chain, M = values of one owner, E = endpoints.

- `compose` runs for every owner (not only endpoints), twice (`model.rs:309-314, 827-829`):
  Θ(Σ_owners chain contributions) = Θ(O·C) for shallow trees, **Θ(D²)** for a chain with
  owners at every level. `compose_family` clones the writer's dotted name per contribution
  (`model.rs:965`), O(D) bytes each.
- Sensitivity worklist: memberships = slots × history = same Θ(Σ chain contributions); the
  C4-R-011 fix made propagation linear in memberships (PERF.md). `contrib_values` allocates a Vec
  per call (`model.rs:885-897`) — constant factor only.
- `resolve_ref`: linear scan of the owner's values, allocating a `String` per comparison via
  `plain()` (`model.rs:361-366`) → **Θ(M²)** for an owner whose M values are referenced from
  blocks/templates. Concrete declaration: `gen.py api OUT refs --values M`. Noisy single runs of
  `-Ztime-passes` `macro_expand_crate` (*noisy, concurrent builds*, `EXP/d17/macro_time.txt`):
  refs vs direct placements 0.110/0.071 s (M=500), 0.221/0.104 s (M=1,000), 0.306/0.299 s
  (M=2,000) — not distinguishable from noise at realistic sizes; quadratic only in theory.
- `flatten`'s ancestor-name check and `Places::owner_var`: O(D) per scope / per reference.
- Emission: per endpoint Θ(depth + inherited contributions + own values); totals Θ(E·C) and
  Θ(Σ depth). The output `Item` tree contains each contribution once (`fallback` is moved, not
  cloned, `model.rs:1013`), so override chains are linear in size but **nested** in depth.
- Generated type names concatenate every ancestor name: Θ(D) characters per type, Θ(D²) total;
  they appear verbatim in rustc messages (see 4.3).

No superlinear term was found beyond the two inherent products of per-endpoint specialization
(E×C and Σdepth) and the theoretical Θ(M²) name resolution. KEEP D1.

### 4.3 Robustness at extreme sizes (C16; measured with `EXP/d17/limits.sh`, control)

| Declaration | Threshold | Failure |
|---|---|---|
| Scope chain depth (`deep --leaf-only`) | 48 OK, **56 fails** on 1.97.1 | rustc E0320 "overflow while adding drop-check rules" for the endpoint type (overflows inside reqwest's `Client` internals through the nested `__parent`/`Arc<State>` chain); span = whole `api!` invocation; the message names a 164-character concatenated type (`DeepL0L1…L55E0`). `#![recursion_limit = "256"]` makes depth 64 compile. Depth 1,000: SIGSEGV on 1.97.1; on nightly it emitted **41.6 GB** of diagnostics (cargo's `output-lib-*` replay file) before failing (depth 300: 1.08 GB). Those target dirs were deleted after recording sizes. |
| Optional overrides of one key in one endpoint (nested `if let … else`) | 500 OK, **1,000 SIGSEGV** on 1.97.1 (nightly: 1,000 OK, 2,000 SIGSEGV) | rustc stack overflow |
| Values on one owner | 2,000 OK; **3,000 SIGSEGV on nightly only** (1.97.1, 1.85 OK up to 4,000) | trigger: the generated `Debug` is one `f.debug_struct(..).field(..)…` receiver chain; a plain-Rust 3,000-deep `.field()` chain (`EXP/d17/crash/chain/chain.rs`) reproduces it on nightly; the statement form does not |

Recommendations: `c16-debug-statements` (generated `Debug` uses `let mut __d = …; __d.field(…);`
statements; +13/−6, tests 101/0, UI stderr unchanged, nightly M=4,000 then compiles) — **CHANGE**,
zero-cost hardening. Depth and override nesting: **DEFER/document** ("about 48 nested scopes with
the default `recursion_limit`"); real APIs are < 10 deep. A macro-side depth error is possible but
the macro cannot see a raised `recursion_limit`, so it would forbid what users can currently opt
into.

### 4.4 Generator and consumers (for the coordinator's timing slot)

`benches/scale/gen.py` v2 on SNAP branch `d17-scale-generator` (patch
`EXP/patches/d17-scale-generator/`, copy `EXP/d17/gen.py`, sha256 `46ec2895…`). Pure function of
its arguments; legacy form `gen.py OUT N CONCORD` is byte-identical to the original for N =
50/200/1,000 (`EXP/d17/legacy_check`).

- **One command:** `python3 -I benches/scale/gen.py suite OUT [--concord PATH]` → a cargo
  workspace: `scale50`, `scale200`, `scale1000` (rest shape), `scale_deep` (depth 32, 2
  endpoints/level), `scale_shared` (200 endpoints, 56 inherited contributions each), binaries
  `use5`/`use100`/`use1000` (5/100/1,000 of scale1000's endpoints, stride `i*1000//K`),
  libraries `fan_a`/`fan_b` (100 endpoints each, 50 shared) and binary `fan_app` (two-crate
  fanout). Every call sets one optional, runs `prepare()` into `black_box`, and boxes `send()` as
  `dyn Future` (code-generated, never polled; no I/O, no tokio).
- `Cargo.lock` is seeded from the Concord workspace lock and settled offline; `suite.json`
  records parameters, selections, per-file sha256 and the lock sha256
  (`83bfbff6…adb04` against this SNAP). Generated twice → identical `suite.json`.
- Built: `cargo build --offline --locked --workspace` (dev) clean with no warnings; all four
  binaries run (exit 0); release build of `use5/use100/fan_app` OK.
- Single-dimension shapes for size studies: `gen.py api OUT {rest,deep,shared,override,optionals,refs} …`.
- `EXP/d17/timing_plan.sh SUITE [run]` prints (dry run) or executes the hyperfine matrix
  (check/dev/release per API crate; edit-API → rebuild each consumer; edit-consumer-only;
  fanout), with `--export-json` per row. **Not executed.** For A/B, generate one suite per
  snapshot (separate OUT ⇒ separate target dirs) and alternate.

## 5. Inline review (3ba81c5; expansion and symbol evidence only, no timing)

- Generic fns (`impl IntoValue`/`AsRef` params: `set_*`, `new` and navigation methods with
  required values, `with_base_url`) are monomorphised at the call site anyway; `#[inline]` there
  only hints LLVM. Non-generic fns where it changes code placement: `prepare`, `send`, `maybe_*`,
  `Debug::fmt`, zero-argument navigation/`new`, the nested `__concord_default_*` fns.
- Release `libscale50.rlib` contains **zero** defined text symbols: nothing is code-generated in
  the API crate, including `send`'s coroutine.
- In consumers `prepare` is **not inlined**: `use100` has 100 out-of-line `prepare` symbols, all
  1,535 bytes (0x5ff), plus 100 `send::{closure#0}`. So `#[inline]` acts as "code-generate in the
  calling crate", not as inlining.
- Fanout: `fan_app` contains **200** `prepare` copies for **150** distinct endpoints — the 50
  shared endpoints are code-generated once per consumer crate.

Doubtful cases: `prepare` with large bodies (455–668 lines at heavy inheritance/depth) and
`Debug::fmt` with many fields: never actually inlined, duplicated per consumer crate. Selective
policy that would be worth testing: **drop `#[inline]` from `prepare` only** (and possibly
`fmt`), keep it on the small non-generic fns. That trades "codegen per (consumer crate × used
endpoint)" for "codegen once per declared endpoint" — PERF.md already shows the latter costs
16.7 s vs 5.9 s for a 1,000-endpoint crate with one 5-endpoint consumer. A second option with no
placement trade-off is shrinking `prepare` (runtime helpers for the 5-line value-segment block,
static literal headers without per-request re-validation). **Measurement that would justify a
change:** the suite's `fan_app` plus a wider fanout (e.g. 4–8 consumer crates sharing 100–500
endpoints) and `use1000`, release, edit-API and edit-consumer rows, current vs
prepare-without-inline, with binary size and `nm` copy counts; change only if a realistic
multi-crate topology improves beyond noise without regressing the single-consumer case. Until
then **KEEP**.

## 6. Hygiene and naming (T01–T03; `EXP/hygiene`, rerun with `EXP/hygiene/run.sh`, rustc 1.97.1)

| Case | Result |
|---|---|
| Raw identifiers as client value (`r#type`), scope (`r#match`), endpoint (`r#async`, `r#yield`), values (`r#in`, `r#ref`, `r#loop`, `r#where`), bare-keyword and qualified holes (`{ref}`, `{match.loop}`, `{client.r#type}`) | PASS (wire URL/header asserted; `Debug` works) |
| Two `api!` in one module, same scope/value names | PASS |
| `api!` inside a fn body; initializer uses a block-local `const` | PASS |
| Library crate (edition 2021) re-exporting a **renamed** Concord and wrapping `api!` in its own exported `macro_rules!`; downstream crate has no `concord` dependency; client name passed as `$name:ident`; user body references a value declared inside the library's macro | PASS (`EXP/hygiene/fwd`) — stronger than the repo's same-crate `tests/hygiene.rs` and `examples/renamed` |
| `#[no_implicit_prelude]` module | PASS |
| Strict lints: clippy pedantic + nursery, rustc `unused_qualifications`, `unreachable_pub`, `missing_docs`, `missing_debug_implementations`, `unused_results`, `elided_lifetimes_in_paths`, `single_use_lifetimes`, `unnameable_types`, … | PASS; only restriction lint `clippy::missing_docs_in_private_items` fires (3 private fields) |
| Cross-invocation collision: `client Shop { scope cart }` + `client ShopCart` | rustc E0428 + 14 cascading errors naming internals (`__ConcordShopCartState`, `__http`, `__base`); primary span = the whole second invocation |
| DTO collision: `pub struct GithubUser` + `client Github { GET user() }` (realistic) | rustc E0428 + 4 cascades (`__parent`); span = whole invocation |
| Generated names in rustdoc | `__Concord*State` and all `__` fields are absent (private + `doc(hidden)`); they do appear in symbols/backtraces (`Arc<scale1000::__ConcordScaleState>::drop_slow`, `drop_glue::<ArcInner<__ConcordScaleS0State>>`) and in collision cascades |
| Macro error side effect | when `api!` fails, the user's `use concord::Text;` is reported as an unused import (no items were emitted) |
| T03 policy | owners accept only docs (`syntax.rs:166-179`); docs on positional values were the only accepted surface that did not render (fixed by `docs-positional`) |

Collisions are inherent to flat concatenated names (D9); the macro cannot see sibling items.
Recommendation: document the naming rule next to the generated-type table and DEFER any renaming
(e.g. a `…Request` suffix for endpoint types) to the DSL/generated-API track, which owns that
trade-off.

## 7. Counterexamples intentionally left unsupported

- A user type whose name equals a generated concatenated name (E0428 cascade remains).
- Scope chains deeper than ~48 without a raised `recursion_limit`; > ~500 nested optional
  overrides of one key; > ~2,500 values on one owner with rustc nightly 1.101 unless
  `c16-debug-statements` is adopted.
- Code-generation duplication across consumer crates in fanout topologies (until measured).

## 8. DEFECTS in the control (with reproduction)

1. **Header name > 65,535 bytes compiles, then `prepare()` panics** ("HeaderName::from_static
   with invalid bytes", http 1.5.0 `name.rs:1254`). Repro `EXP/hygiene/long_header` (65,535-byte
   name builds; 65,536-byte name panics; both asserted). Fix `defect-header-name-len`: compile
   error "header field names are limited to 65,535 bytes" (`model.rs` `key()`, +8); verified.
2. **Docs on required values / required bodies are silently dropped** (and the project's own
   example loses one). Repro `EXP/hygiene/docs` + rustdoc JSON. Fix `docs-positional`.
3. Scope navigation doc line merged into the user's last paragraph. Same repro and fix.
4. `with_base_url`'s generated doc omits the port-0 rule (`emit.rs:278-280` vs
   `__private.rs:55`); `encode_body` duplicated doc line (`__private.rs:119-121`). Trivial.
5. Long declarations crash or explode rustc (§4.3): depth ≥ 56 → E0320 (≥ 1,000 SIGSEGV, or
   41.6 GB of diagnostics on nightly); 1,000 nested optional overrides → SIGSEGV; ≥ 3,000 values
   on one owner → SIGSEGV on nightly via the generated `Debug` chain. Limits far above realistic
   use; only the last has a zero-cost mitigation.

## 9. Files

- Report: `/home/canarit/projects/concord_v4_refine_work/reports/compiler.md`
- Patches (one per SNAP branch, `git am`-able on 8f7adc2):
  `/home/canarit/projects/concord_v4_refine_work/exp/compiler/patches/{d16-emit-context,d16-krate-param,d17-scale-generator,c16-debug-statements,defect-header-name-len,docs-positional}/`
- D16: `…/exp/compiler/d16/expand_all.sh`, `…/d16/expanded-{control,variantA,variantB}/`
- D17: `…/exp/compiler/d17/{gen.py,sizes.py,fnsizes.py,macro_time.sh,limits.sh,timing_plan.sh}`,
  results `sizes/results.json`, `sizes.log`, `fnsizes.txt`, `macro_time.txt`, `limits.txt`,
  generated suite `d17/suite/` (with `suite.json`), nightly crash trigger `d17/crash/chain/`
- Hygiene: `…/exp/compiler/hygiene/{run.sh,out/,raw_idents,two_apis,in_fn,fwd,no_prelude,collide_cross_api,collide_dto,strict_lints,long_header,docs}`

Rerun (offline; SNAP `master` = control, or check out a prototype branch first):
```text
EXP=/home/canarit/projects/concord_v4_refine_work/exp/compiler
SNAP=/home/canarit/projects/concord_v4_refine_work/snapshots/compiler-8f7adc2
$EXP/d16/expand_all.sh $SNAP /some/out && diff -r $EXP/d16/expanded-control /some/out
python3 -I $EXP/d17/gen.py suite /some/suite --concord $SNAP/crates/concord
python3 -I $EXP/d17/sizes.py $SNAP ; $EXP/d17/limits.sh $SNAP ; $EXP/hygiene/run.sh
$EXP/d17/timing_plan.sh /some/suite          # dry run; append `run` in the exclusive slot
```
