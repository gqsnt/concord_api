# V4 status

Branch `concord-v4` (worktree `/home/canarit/projects/concord_api_v4`), based on V3 `a492d36`.
V3 preserved at tag `concord-v3-preserved`, `origin/main`, `origin/concord-v3` and local
`concord-v3`. Nothing pushed or published.

## Gates

| Gate | Implemented | Tested | Reviewed | Accepted |
|---|---|---|---|---|
| 0 Preservation and baseline | yes (`evidence/BASELINE.md`) | V3 build/test baseline recorded | — | yes |
| 1 Research and blind design challenge | `DECISIONS.md`, research reports | — | blind Codex review before design; 4 independent Claude tracks | yes |
| 2 Vertical slice | `2639b39` | `slice.rs` 7/7 | Codex Gate 2 (7 findings, all fixed) | yes |
| 3 Values and composition | `24717e8` → `33a88d9` | values, composition, hygiene, ui | Claude adversarial (10 fixed, 1 documented); Codex Gate 3/4 (6 new findings: 5 fixed, 1 resolved by amending the contract); Claude matrix audit (gaps closed) | pending final Codex re-review |
| 4 Bodies, responses, sensitivity | `24717e8` → `33a88d9` | bodies, representations, sensitivity, tls | as above | pending final Codex re-review |
| 5 Hardening | docs, examples, perf, CI file, MSRV | `scripts/verify.sh --msrv` all PASS at `33a88d9` | final Codex re-review running | pending |

Final-gate details, SHAs and exact results: `evidence/GATES.md`. Review triage:
`evidence/reviews/TRIAGE.md`.

## Platform coverage

- Linux x86_64, rustc 1.97.1: all gates run.
- Linux x86_64, rustc 1.85.1 (declared MSRV): runtime and example suites run; UI expectations are
  pinned to 1.97.1 (5 of 37 cases differ only in rustc's wording on 1.85).
- **Windows: NOT_RUN** (no Windows environment). A CI workflow with a Windows job exists but has
  not been executed. **macOS: NOT_RUN.**

## Open risks

1. Per-endpoint code generation is ≈ 143 expanded lines per endpoint (linear); measured up to 1,000
   endpoints (1.36 s incremental `cargo check`). Very large APIs in release builds are unmeasured.
2. Request construction is 110–250 ns slower than a hand-written builder (owned `String`s per value,
   handle clones); execution cost is indistinguishable over loopback.
3. Same-typed positional required values can be swapped silently (newtypes are the mitigation).
4. Lower transport layers' error text is outside Concord's control; only sensitive host labels
   trigger hiding the `source()` chain. Secrets in a runtime base URL are not redacted.
5. Two rustc trait errors point at the whole `api!` invocation (cost of the `$crate` wrapper).
6. UI tests depend on the exact rustc version.
7. Streaming request bodies are only reachable natively (`into_parts`), and untested here.
