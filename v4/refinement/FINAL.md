# Concord V4 refinement — final report

**The refinement is implemented and verified on Linux x86_64 at code revision
`b0fe26e4ec3d0c2ac7e1ebb4216d05a38a4aff4e`** (branch `concord-v4-refine`, worktree
`/home/canarit/projects/concord_api_v4`). Fresh locked verification (`scripts/verify.sh --msrv`:
fmt, clippy `-D warnings`, four test configurations, three strict rustdoc configurations, MSRV
1.85.1 runtime and example suites) passed on `b0fe26e` for the coordinator and, independently, for
the Codex focused re-review; the Codex final review ran the same gates, also passing, on its
predecessor `0fe60e1`. The independent reviews found no blocker or high-severity defect. Their last
open point, the claim-to-test matrix (R2), was corrected in record-only commits and confirmed
RESOLVED by a focused read-only Codex check of `bbbeca1`; the record wording that check flagged is
corrected in the commit that follows it (all recorded in `REVIEW_TRIAGE.md`). **Windows and macOS were not run** for any refined commit: there is no Windows
host, VM or wine here, and CI needs a push, which was not authorized. Nothing was pushed or
published.

## Identity

| Item | Value |
|---|---|
| Baseline (alpha) | `8f7adc2019f3ee31af087c69286d559bca1c173a`, tag `concord-v4-alpha1-baseline` (= `concord-v4` = `origin/concord-v4`) |
| Verified code revision | `b0fe26e4ec3d0c2ac7e1ebb4216d05a38a4aff4e`, tree `3e0540f03a995510148e245488b8b06b3a31d610` |
| `Cargo.lock` | sha256 `1880bdf8dfe1402b2e823ebc893b4f1a37025b5e026c73c2372ffccdfc1e8269` (unchanged since `0fe60e1`) |
| Later commits | record-only, under `v4/` (this file, `TEST_MATRIX.md`, `refinement/` records, receipts, archived reviews); `git diff --stat b0fe26e HEAD -- . ':!v4'` is empty |
| Receipt | [`evidence/verify_b0fe26e.txt`](evidence/verify_b0fe26e.txt): 11/11 PASS; 443 default-workspace tests, 349 / 365 / 351 isolated feature sets, 368 + 69 on 1.85.1 (counts overlap) |
| Preserved | V3 branch `concord-v3` and checkout `concord_api`, tag `concord-v3-preserved`, worktree `concord_api_v3ref`, alpha branch `concord-v4`, untracked first-campaign inputs in the V4 worktree |

## What changed for users (before → after)

**Declaring an API.** Representations are named after the payload; an optional override says what
absence means.

```text
// alpha (8f7adc2)
POST create_item(body item: Json<NewItem>) -> Json<Item> { path "items"; }
GET product(id: String, version?: String) -> Json<Product> {
    headers { override "x-api-version" = version; }
}

// refined
POST create_item(body item: NewItem via Json) -> Item via Json { path "items"; }
GET product(id: String, version?: String) -> Product via Json {
    headers { override "x-api-version" = version else inherit; }
}
```

**Calling it.** Required client values are named; every value can be replaced; signatures show
payload types.

```text
// alpha: positional, same-typed values swap silently; rustdoc shows a projection
let api = ShopApi::new(http, "t0k");
// send(self) -> Result<<Json<Item> as Decode>::Output, Error>

// refined
let api = ShopApi::new(http, ShopApiConfig { token: "t0k".into() });
let api = api.set_token(rotated);           // rotate without losing with_base_url
// send(self) -> Result<Item, Error>
```

Excerpts of `examples/shapes` before and after; the compiled sources are the reference.

Other user-visible changes:
- **Clone:** an endpoint request is `Clone` exactly when its data is. Move-only values are
  accepted, and body requests clone.
- **Removed panics, now errors:**
  - an over-long header name is a compile error;
  - a full header map is a build error;
  - a `Display` that fails is a build error naming the placement;
  - `Debug` shows `<Display failed>`.
- **Native escape:** `Decoder::finish` completes a request you execute yourself.
  `Error::into_response()` returns `Result`, and `BodyError` implements `Error`.
- **Attributes:** `#[cfg]` and `#[deprecated]` work on scopes and endpoints.
- **Composition:** exact replacement works with a direct placement, and the cross-name header order
  claim was withdrawn.
- **Generated rustdoc,** derived from the resolved model:
  - each endpoint gets a request summary: method, URL template, body, output, and query and header
    keys;
  - each value says where it is sent, whether it is sensitive, its initializer and when it runs,
    and its per-call inherit/omit/replace behaviour.
- **Diagnostics** point at the declared type or representation and name the fix, alpha spellings
  included.
- **Docs:**
  - task-first README;
  - quick start, compiled as `examples/quickstart`;
  - author and caller guides, recipes, a diagnostics gallery and a maintainer guide;
  - an alpha → refined migration section;
  - 29 application-owned native recipes in `examples/native`.

## Decisions (details: [`DECISIONS.md`](DECISIONS.md))

| Disposition | Decisions |
|---|---|
| CHANGE | D01 named root config + setters for every value; D04 `else inherit`, `remove` first; D06 `T via R`; D08 conditional `Clone`; D12 native header/format boundaries; D13 `Decoder::finish`, `into_response -> Result`; D14 `cfg`/`deprecated`; D16 explicit crate path; D18 docs |
| KEEP | D02 `set_`/`maybe_` (docs fixed); D03 eager defaults, no reset; D05 query slot order (header contract narrowed); D07 `Display` + newtypes; D09 copy-on-write handles (structural wording); D10 wrapper for non-wire state; D11 strict static base subset (+ consistency test); D15 safe errors, no `source()`; D17 per-endpoint codegen and blanket `#[inline]` |
| REJECTED | scoped representation defaults; re-evaluating or retained-baseline reset; lazy defaults; plain-`override`-is-exact; `Prepared::request_mut`; a typed error-document framework; a wire-value trait; url in the proc macro; a fallible constructor |
| DEFERRED | per-placement formatting adapter; read accessors / `local` values; an `ErrorKind::Encode` |

## Verification and platforms

| Gate | Status | Evidence |
|---|---|---|
| Linux x86_64, rustc 1.97.1 | PASS | `evidence/verify_b0fe26e.txt`; Codex re-review `reviews/codex/rereview_b0fe26e_evidence/verify.log` |
| MSRV 1.85.1 | PASS (runtime and example suites; UI snapshots pinned to 1.97.1) | same receipts |
| Feature isolation | PASS | script feature steps; independent separate-manifest consumers in the Codex final review and re-review |
| Windows | **BLOCKED / NOT_RUN** | no host, VM or wine; CI not executed (push not authorized) |
| macOS | NOT_RUN | — |
| Rendered rustdoc visual QA | BLOCKED | generated HTML text inspected; browser rendering failed in the reviewer sandbox |
| Packaging | PASS (listings only) | `cargo package --list`: licenses, exact `=4.0.0-alpha.1` macro pin; nothing published. The `concord` package includes its test fixtures, including the self-signed test TLS key material: exclude before any publication |

Test evidence beyond counts:
- **Composition oracle:** 1,728 chains and 1,233 checks.
- **Sensitivity oracle:** 252 cases with wire-side positive controls.
- **Mutations killed:** 9/9 in M4 and 5/5 in the final review.
- **Native recipes:** 29 loopback tests.

See `TEST_MATRIX.md` (status bound to the receipt) and `evidence/m4_mutations.md`.

## Performance (details: [`PERF.md`](PERF.md); raw runs in `evidence/perf/`)

Measured revisions: alpha `8f7adc2`; refined `de6471f` (and its no-inline variant) for the
inline experiment; `d1844be` for the final comparison and the runtime benchmark. The delivered
`b0fe26e` was **not re-timed**: it differs from `d1844be` in generated documentation notes and doc
comments (no runtime logic), which can affect compile cost a little.

- **`#[inline]` kept** (measured on `de6471f`). Removing it from `prepare` made release builds of
  the declaring crate and API-edit rebuilds 1.43–2.29× slower (`use1000`: 0.98×) and consumer-only
  release rebuilds 12–25% faster.
- **A regression versus the alpha was found, attributed and reduced, not hidden.** Per-contribution
  `const` header validation and quadratic doc-note generation were fixed.
- **Remaining accepted cost** (`d1844be` vs alpha): in the declaring crate, `check` is +27% (1,000
  endpoints) and +42% (shared-contribution shape); release is +18–29%.
- **Consumers** (measured workloads): consumer-only rebuilds 0.94–1.06×. Rebuilding a consumer after
  an API edit costs +10–15% in release (`use5`, `use100`, fanout; 1.00× for `use1000`) and +25% in
  dev (`use100`), mostly the declaring crate's own extra cost. Loopback execution is broadly similar.
- **Request construction:** +105 ns for nine query values, attributed to fallible formatting.
- **Conditions:** one machine with the desktop in use; compile timings are three hyperfine samples
  per row (runtime: medians of nine runs, two rounds); differences under ~5% are noise.

## Reviews (details: [`REVIEW_TRIAGE.md`](REVIEW_TRIAGE.md); raw reports in `reviews/codex/`)

| Review (Codex `gpt-6.1-sol`, `xhigh`, observed in session logs) | Result |
|---|---|
| Design challenge pass 1 (blind) and pass 2 (source, execute) | answered row by row in `DECISIONS.md`; capacity panic fixed |
| Milestone `6604a53` (execute) | F-01 multipart escaping, F-02 raw-keyword root, F-03 strict no-default rustdoc → fixed and independently confirmed in the final review; F-04 stale contract/migration → not fully closed there (carried into R2/R3), closed by `b0fe26e` and the record commits, confirmed by the re-review (R3) and the R2 check |
| Final acceptance `0fe60e1` (execute) | REVISE: R1 generated default wording, R2 matrix, R3 prose; no new high-severity defect |
| Focused re-review `b0fe26e` (execute) | R1, R3 RESOLVED; R2 narrowed to receipt binding and the G2 exemption → corrected in `bbbeca1` |
| Focused R2 check `bbbeca1` (static, read-only) | R2 RESOLVED; files outside `v4/` blob-identical to `b0fe26e`; four wording issues in this report (review attribution, performance revisions, verification identities, F-04 closure) → corrected in the next record commit |

Also used:
- **Claude Opus 5.5 specialist tracks at `xhigh`:** authoring, calling, core and compiler.
- **Implementation branches at medium effort:** tests and native recipes.
- **Docs at `xhigh`.**
- **Two fresh-use trials (agent simulations, not human studies):** the control (alpha) and the
  refined surface, on the same neutral task. All of the refined surface's predictions matched and 9
  of 10 tasks compiled first time. Key rotation now works, and the participant found the
  inherit/omit/replace pattern in the guides rather than only in SEMANTICS. Still reported: the
  doubled-`Option` (`set_x(None)` versus `maybe_x(None)`) remains surprising to read, and the
  documented IP-base/subdomain rule caught the participant once.

## Migration

See [`docs/MIGRATION.md`](../../docs/MIGRATION.md), section "From the V4 alpha". The edits:
- **Root constructor:** `new(http, a, b)` becomes `new(http, ClientConfig { .. })`.
- **Representations:** `Json<T>`, `Text` and `Binary` become `T via Json`, `String via Text` and
  `Bytes via Binary`.
- **Custom representations:** `Encode`/`Decode` move from associated types to generic traits on a
  marker. `DISCARD` is gone: an endpoint without `->` drains.
- **Prepared types:** `Prepared<R>` and `Decoder<R>` become `Prepared<T>` and `Decoder<T>`.
- **Optional overrides** need `else inherit`.
- **`remove`** acts first within an owner.
- **`into_response()`** returns `Result`.

The compiler names most of these edits. No second legacy parser was kept.

## Remaining limitations

| Limitation | Kind |
|---|---|
| Auth flows, retries, rate admission, pagination traversal, transport configuration | product boundary (application code; `examples/native` shows routes) |
| Native `Request`/`Url`/`Response` `Debug`, `Error::transport()`/`body_source()`, custom `BodyError` text, redirects/logs, the runtime base URL are outside Concord's redaction | product boundary, documented |
| Same-typed scope/endpoint positional arguments can still be swapped; same-typed config fields can be assigned crosswise | accepted trade-off (newtypes are the remedy) |
| Interleaved repeated query keys, multipart and streaming *request* bodies are native routes | accepted trade-off |
| Compile cost of the declaring crate above the alpha; ~100 ns more per nine formatted values | accepted trade-off (`PERF.md`) |
| The conditional `Clone` relies on rustc accepting trivially-false bounds under a binder (1.85.1–1.101 checked) | accepted risk (rust-lang/rust#48214 moves toward accepting such bounds) |
| Over-long header name diagnostic spans the whole invocation; rustc-origin trait-error wording varies by compiler | diagnostic limit |
| Deep scope chains (> ~48) and ~1,000 nested overrides hit rustc limits; a declaration with ~24,575 distinct header names reaches a hyper-util panic at send | resource boundary, documented |
| Windows, macOS, HTTP/2, TLS variants, allocation counts, rust-analyzer, LTO, rendered docs QA | unmeasured / not run |

No known defect is open. Remaining findings were record wording, corrected as listed in `REVIEW_TRIAGE.md`.

## Reproduce and continue

```text
git -C /home/canarit/projects/concord_api_v4 log --oneline 8f7adc2..concord-v4-refine
scripts/verify.sh --msrv
python3 -I tests-gen/oracle_composition.py --check && python3 -I tests-gen/oracle_sensitivity.py --check
python3 -I benches/scale/gen.py suite OUT && benches/scale/ab.sh RESULTS OUT ...
```

Start with `README.md` and `docs/QUICKSTART.md`, and for maintenance `docs/MAINTAINERS.md`. Live status
is in `v4/refinement/STATUS.md`. The next owner-only steps are pushing a branch so the CI Windows and
MSRV jobs run on the refined code, then deciding whether to merge into `concord-v4` or `main`.
