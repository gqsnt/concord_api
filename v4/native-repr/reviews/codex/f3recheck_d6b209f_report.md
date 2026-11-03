**Verdict: REVISE — the original findings are resolved, but one new P3 migration-hint defect remains.** This was a narrow re-check, not a full audit. [Triage and evidence](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/target/probes/triage.json).

| Finding | Status | Evidence |
|---|---|---|
| F3 | **RESOLVED** | `codec::Text` retains custom request/response behavior. |
| F3a | **RESOLVED** | Scoped, cfg-disabled, and commented imports preserve codec selection. |
| F3b | **RESOLVED** | Shadowed `concord` modules, aliases, and macro reexports preserve custom behavior. |
| F3c | **RESOLVED** | Bare `text`/`json`/`bytes` remain unchanged and are reported; documented `self::…` edits pass. |
| F3d | **RESOLVED** | Absolute body and response paths compile and pass wire checks. |

**New F3e — P3, NOT RESOLVED:** migration guidance drops a required raw-identifier prefix. With `type r#type = concord::Text`, both the script and compiler suggest `self::type b: String`. Applying that suggestion fails with “expected identifier, found keyword `type`”; `self::r#type b: String` compiles. The defect occurs in [compiler hint construction](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/crates/concord-macros/src/syntax.rs:168) and [script guidance](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/scripts/migrate_via.py:46).

[Executable reproducer](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/target/probes/raw-keyword/src/bin/keyword_suggested.rs):

```bash
cargo +1.85 check --locked --offline \
  --manifest-path target/probes/raw-keyword/Cargo.toml \
  --bin keyword_suggested
```

This fails on all three tested toolchains; replacing the binary name with `keyword_corrected` passes.

Personally executed on **stable 1.99.0, pinned 1.97.1, and MSRV 1.85.1**, per toolchain:

- Four original reproducers: **4 passed**.
- Paired wire matrix: **38 passed** — 35 automatic migrations and three documented manual lowercase edits.
- Cargo rename/reexport probe and four absolute-path compiler probes: **passed**.
- **128 leftover-`via` diagnostics** checked, including the raw-keyword defect above.

Also executed the script self-test (**11 passed**) and the requested `formats`/`ui` command **once**, on pinned 1.97.1: **27 format tests and 66 UI cases passed**. [Results](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/target/probes/test-summary.json).

Effective session metadata confirms **`gpt-6.1-sol` / `xhigh`**, execute mode, workspace-write, approval policy `never`, and private `CARGO_HOME`. All builds ran offline. [Settings receipt](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/target/probes/environment-receipt.json).

Historical F1/F2/F4 dispositions were **inspected only**. Wire controls used current explicit formats; no older library accepting `via` was compiled.

All **1,211 original files and 1,212 tracked files remain unchanged**. Source bytes match the supplied manifest/tree; the original commit object is unavailable locally. No probe processes or servers remain. No external services, push, publish, global-setting changes, or sandbox bypass. [Integrity receipt](/home/canarit/projects/concord_v4_native_work/snapshots/f3recheck-d6b209f/target/probes/source-integrity.json).