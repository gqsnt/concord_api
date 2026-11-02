# V4 status

> **Historical: the V4 alpha (`8f7adc2`).** The refinement campaign's record is in
> [`refinement/`](refinement/) (`STATUS.md`, `FINAL.md`); the current campaign (native header
> values, body/response formats) is in [`native-repr/`](native-repr/) (`STATUS.md`, `FINAL.md`).

Branch `concord-v4` (worktree `/home/canarit/projects/concord_api_v4`), based on V3 `a492d36`.
V3 preserved at tag `concord-v3-preserved`, `origin/main`, `origin/concord-v3` and local
`concord-v3`. Nothing pushed or published.

## Gates

| Gate | Implemented | Tested | Reviewed | Accepted |
|---|---|---|---|---|
| 0 Preservation and baseline | yes (`evidence/BASELINE.md`) | V3 build/test baseline recorded | — | yes |
| 1 Research and blind design challenge | `DECISIONS.md`, research reports | — | blind Codex review before design; 4 independent Claude tracks | yes |
| 2 Vertical slice | `2639b39` | `slice.rs` 7/7 | Codex Gate 2 (7 findings, all fixed) | yes |
| 3 Values and composition | `24717e8` → `caf7cb3` | values, composition, hygiene, ui | Claude adversarial (10 fixed, 1 documented); Codex Gates 3/4 and final (all findings fixed or resolved by correcting the spec); Claude matrix audit (gaps closed); focused Codex re-review: resolved | yes, on Linux |
| 4 Bodies, responses, sensitivity | `24717e8` → `caf7cb3` | bodies, representations, sensitivity, tls, heap check | as above | yes, on Linux |
| 5 Hardening | docs, examples, perf, CI file, MSRV, locked verification; `#[inline]` on generated methods (`3ba81c5`) | `scripts/verify.sh --msrv` all PASS at `3ba81c5` | Codex final review on `33a88d9`; focused re-review on `8d6ee22`: no blocker or high finding; last residual (documentation/script) fixed in `8ae19ec` and the `#[inline]` change (`3ba81c5`, attributes only) not independently re-reviewed | yes, on Linux; Windows and macOS unverified |

**Acceptance statement.** V4 is accepted for Linux x86_64 (rustc 1.97.1, MSRV 1.85.1): every
high-severity finding from the independent Codex and Claude reviews was reproduced, fixed and
confirmed resolved by re-review; the remaining open items below are documented trade-offs or
unmeasured areas, not defects. It is not accepted for Windows or macOS (never executed).

Final-gate details, SHAs and exact results: `evidence/GATES.md`. Review triage:
`evidence/reviews/TRIAGE.md`.

## Platform coverage

- Linux x86_64, rustc 1.97.1: all gates run.
- Linux x86_64, rustc 1.85.1 (declared MSRV): all runtime and downstream suites run; UI
  expectations are pinned to 1.97.1 (6 of 41 cases differ only in rustc's wording on 1.85).
- **Windows: NOT_RUN** (no Windows environment). A CI workflow with a Windows job exists but has
  not been executed. **macOS: NOT_RUN.**

## Open risks

1. Per-endpoint code generation is ≈ 143 expanded lines per endpoint (linear). At 1,000
   endpoints, incremental `cargo check` takes 1.35 s and a release rebuild of the declaring crate
   5.9 s (was 16.7 s before generated methods became `#[inline]`, `3ba81c5`); crates that call
   endpoints now compile the ones they use. Release builds of binaries using hundreds of endpoints
   are unmeasured.
2. Request construction is 110–250 ns slower than a hand-written builder (owned `String`s per value,
   handle clones); execution cost is indistinguishable over loopback.
3. Same-typed positional required values can be swapped silently (newtypes are the mitigation).
4. Lower transport layers' error text is outside Concord's control, so `Error` has no `source()`
   chain (explicit `transport()` / `body_source()` only); applications that print those accessors
   must handle redaction themselves. Secrets in a runtime base URL are not redacted.
5. Two rustc trait errors point at the whole `api!` invocation (cost of the `$crate` wrapper).
6. UI tests depend on the exact rustc version.
7. Streaming request bodies are only reachable natively (`into_parts`), and untested here.
