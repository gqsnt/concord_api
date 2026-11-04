# Concord 4.2.0 — final report

**State: ACCEPTED_NATIVE_LINUX_AND_WINDOWS** for the code at `ed6703a` (plus the records-only receipt
that adds this file). Mandatory F01–F04, M01–M03, T01 resolved and M04 retired; all six stages
dispositioned; native Linux and Windows gates and independent reviews completed on identified
sources. This is not publication, a merge into `main`, a release tag, security certification or a
claim of exhaustive correctness.

## 1. Identity

| Item | Value |
|---|---|
| Branch / worktree | `concord-v4.2` / `/home/canarit/projects/concord-v42/candidate` (pushed to `origin/concord-v4.2` for CI only, as authorized) |
| Starting point | `07dcdd03c61a1822341e92d3820e66117f1aef14` (main; records-only successor of `v4.1.0` = `afcec40`) |
| Last code/records commit verified by the gate, CI and the final review | `ed6703a29d6f798f13e06379d488feeac33410f4` |
| Records-only receipt | the commit adding this file, on top of `ed6703a` (only `v4/release-4.2/` changes; checked separately) |
| Working tree | clean |
| Packages | `concord`, `concord-core`, `concord-macros`, `concord-extras` 4.2.0; internal requirements `=4.2.0`; `publish = false`; Rust 1.99.1 minimum and pin |
| Locks | workspace `Cargo.lock` and `checks/consumers/facade-gzip/Cargo.lock`: only the four local versions changed; no registry change |

## 2. Findings

| ID | Baseline (4.1 control) | Fixed assertion (candidate) | Review |
|---|---|---|---|
| F01 | `same_forward` E0277 | declaration compiles; non-Clone clone call E0599; Clone requests clone | no finding in 10 reviews |
| F02 | type-macro output "expected curly braces" | `()`/`[]`/`{}`, absolute/qualified/raw paths decode | no finding |
| F03 | advice `"a::b"` (6 → 4 bytes) | advice keeps literals and macro input token for token; forwarded fragments named `T`; generated docs faithful; forwarded fragments mean what Rust says | widened through 10 independent review rounds, final ACCEPT |
| F04 | advice `body type` | advice `body r#type?: String`, applied compiles | no finding |
| M01 | S3 pair unequal (Accept) | 47 registered comparisons equal at the prepared stage; negative controls detect differences | via final reviews |
| M02 | renamed example E0432 alone | 3 passed alone; every example tested alone in the gate | via final reviews |
| M03 | comment-only gate accepted | structural guard; noexec, prefixed `set`/`exit`/`exec` refused | 2 findings fixed, final ACCEPT |
| M04 | build failure counted as kill | helper retired, refuses to run | via final reviews |
| T01 | head-check mutant survived native suite | Metadata-before-withheld-body test; mutant caught | via final reviews |

Details: [RESOLUTIONS.md](RESOLUTIONS.md); review dispositions: [REVIEWS.md](REVIEWS.md).

## 3. Documentation and coverage

Corrections (HTAB, Status-or-Metadata recovery, runtime base escapes and whole-segment rule, Text's
policy, diagnostic spans/cascades/recursion limits, compiler vintages, `&HeaderValue`), task notes
and a compiled maintenance example; two documentation-first trials (agent simulations). Linked
follow-ups: D16-sol, D22-terra (inside O01), I20-A, I22-A/P08-A (packaged consumer from a deep
non-ASCII path, run natively on Windows CI as part of `package-check`), H05 (RV04). Original
statuses are unchanged. Still unrun: broader cancellation/resource census, macOS, wasm,
HTTP/2/3, dedicated security assignments (NOT_ASSESSED).

## 4. Performance

O01 native header planning and O02 metadata materialization: **DEFER_WITH_EVIDENCE** (lane,
coordinator, independent review); nothing adopted. M01: affected S3 IDs re-measured; historical
ratios kept with a notice. [PERF.md](PERF.md).

## 5. Platform × compiler × features × source

| Platform | Source | Compiler | Command | Result |
|---|---|---|---|---|
| Local Linux x86_64 | `07dcdd0` (baseline) | 1.99.1 | `verify.sh --package --bench` (35 steps) | 35/35, 651 tests |
| Local Linux x86_64 | `9c32294`, `a6b78a1`, `4619334`, `ed6703a` | 1.99.1 | `verify.sh --package --bench` (37 steps) | 37/37 each; 676 workspace tests; 126 isolated-example tests |
| CI Linux (ubuntu-latest) | `9c32294`, `a6b78a1`, `4619334`, `ed6703a` | 1.99.1 | same | success ×4 (final: run 41786978999, job 126431257072) |
| CI Windows (windows-latest), native | `9c32294`, `a6b78a1`, `4619334`, `ed6703a` | 1.99.1 | same | success ×4 (final: run 41786978999, job 126431257049: 37 steps, 676 workspace tests, 126 isolated-example tests) |
| CI nightly lane (non-blocking) | same four | nightly | `verify.sh --nightly-only` | success ×4 (final: job 126431256718, 16 checks) |

Feature-isolated lanes (no-default, json, form, extras, reqwest-defaults, core/extras subsets),
six separate consumer workspaces, fresh 4.2.0 package extraction and packaged consumers run inside
the gate on both platforms.

## 6. Migration

No edit needed for ordinary use: same DSL, crates, codec contracts, policies, request types,
`IntoValue` and toolchain. Newly accepted: type macros as output types; same-printed cross-crate
types with mixed `Clone`; forwarded `dyn A + B` behind `&`. Changed meaning, to Rust's: expression
fragments forwarded by your own `macro_rules!` into a type or initializer (`Payload<{ $e * 2 }>`,
`= $e * 2` with `$e = 1 + 2` is now 6). Advice and generated docs are exact. Contributors: the gate
tests each example alone and inspects the workflow structurally. [MIGRATION.md](../../docs/MIGRATION.md#from-41-to-42),
[CHANGELOG.md](../../CHANGELOG.md).

## 7. Limitations, authorizations, resources

- Known boundary: a forwarded `let` statement fragment inside an initializer block is rejected by
  the initializer parse (as on 4.1).
- Not done: publication, merge into `main`, release tag (each needs separate authorization);
  visual rustdoc QA (HTML text extraction only); cold-cache/other-host performance.
- Workflow guard: static inspection only; CI logs are the execution evidence.
- Resources: the campaign directory `/home/canarit/projects/concord-v42/` peaked at about 203 GiB
  (≈ 218 GB decimal), **transiently above the 200 GB ceiling**, before about 160 GiB of finished
  build outputs (Cargo target directories of merged lanes and finished reviews, each carrying
  `CACHEDIR.TAG`) were recycled; it is now about 72 GiB. Free space stayed above 260 GiB. Logs,
  reports, fixtures, worktrees, the gate's final build and the 4.1 review tree were kept; user
  caches untouched; no campaign process is running.
- Coordinator settings: Claude Opus 5.5 (`claude-opus-5-5`) requested at XHigh; the session's
  effective effort is not observable from inside it.
