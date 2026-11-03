Original source SHA: d6b209f38c50402f3d526aa2f037907542748eb1
Independent snapshot HEAD: 46c3da64ce26beff5e62c671ec1b1bf3d44e502b
Review mode: execute; requested model: gpt-6.1-sol; requested effort: xhigh.
Verify effective settings; report personally executed versus inspected-only evidence.
Never push, publish, modify global settings, bypass sandbox, or claim acceptance before triage.

## Supplied document: prompt.md

# Codex task — narrow executable re-check of F3b–F3d on `d6b209f`

Your narrow check of `d591fe7` (attached as `f3check_d591fe7_report.md`) resolved F3a and found
F3b (shadowed `concord` paths turned into words), F3c (a bare lowercase downstream type kept
verbatim becoming a built-in word) and F3d (absolute body format paths rejected). The fix
(`d6b209f`): `scripts/migrate_via.py` now only moves the path and never turns a path into a word;
it leaves a bare lowercase name (`via text`, `via r#json`) unmigrated and reports it; the
compiler's leftover-`via` error names the path as written (`self::name` for a bare lowercase
name); the parameter lookahead in `crates/concord-macros/src/syntax.rs` accepts `::c4::Text b: T`.
Read those files, `docs/MIGRATION.md` ("From `via` to formats"), `docs/DIAGNOSTICS.md` ("A leftover
`via`"), `crates/concord/tests/formats.rs::migrated_marker_paths_behave_like_the_words` and the
triage at the end of `v4/native-repr/REVIEW_TRIAGE.md`.

Execute on stable and 1.85: your F3, F3a, F3b, F3c and F3d reproducers and your 38 paired wire
checks (re-created as needed), the script's `--self-test`, the leftover-`via` hint for the same
inputs, and `cargo test --locked -p concord --test formats --test ui` once. Report RESOLVED / NOT
RESOLVED per finding with evidence, any new finding (P0–P3) with a reproduction, your effective
settings as observed, and a verdict. This is a narrow check of one area, not a full audit. No
push, publish, global-setting change or sandbox bypass; leave no server running.


## Supplied document: f3check_d591fe7_report.md

**Verdict: REVISE. F3: NOT RESOLVED. F3a: RESOLVED.**

The original `codec::Text` reproducer and the scoped, cfg-disabled and commented-import reproducers now pass. Two additional semantic defects remain:

- **F3b — P2: shadowed `concord` paths.** [`word()`](scripts/migrate_via.py:39) converts `concord::Text` to `text` even when `concord` names a local module or import alias. Both versions compile, but migration changes the body from `CUSTOM:value` to `value`, the content type, and the decoded response. Json, Binary and macro-reexport variants also fail.
- **F3c — P2: bare names become reserved words.** Keeping a downstream type named `text`, `json` or `bytes` verbatim moves it into built-in format position. For example, `via text` becomes `text b: String`, silently selecting Concord’s marker. These cases also change request and response behaviour.
- **F3d — P3: absolute body paths fail compilation.** Migrating `via ::c4::Text` produces `::c4::Text b: String`, rejected with “expected identifier” by [parameter lookahead](crates/concord-macros/src/syntax.rs:951). Absolute response paths compile. This is newly observed compatibility evidence, not proof that this fix introduced it.

Personally executed on **Rust 1.97.1 and 1.85.1**:

| Check | Result |
|---|---|
| Four original F3/F3a reproducers | 4 passed per toolchain |
| 38 paired wire checks | 30 passed, 8 semantic failures per toolchain |
| Actual Cargo rename and reexports | Passed per toolchain |
| Absolute-path compiler checks | Body failures confirmed; response controls passed |
| Migration self-test | 9 passed |
| Requested `formats` suite, once on stable | 27 passed |

See [stable wire results](target/probes/matrix-stable.txt), [MSRV wire results](target/probes/matrix-msrv.txt), [formats output](target/probes/formats-stable.txt) and [triage](target/probes/triage.json). Semantic controls used explicit format paths; I did not compile an older library accepting `via`.

Effective settings observed: execute mode, workspace-write sandbox, approval policy `never`, private `CARGO_HOME`, initially absent review-build directory. **Requested `gpt-6.1-sol` / `xhigh` could not be independently verified** because current runtime model/effort metadata was unavailable. [Environment receipt](target/probes/environment-receipt.json).

Prior F1/F2/F4 dispositions and historical verification claims were inspected only.

All **984 tracked files remain unchanged**. Independent probe servers were joined; no probe processes remain. No external services, push, publish, global-setting change or sandbox bypass. [Integrity receipt](target/probes/source-integrity.json).

## Supplied document: ENV.md

# Execution environment for this run

- Sandbox: workspace-write confined to this snapshot, network access enabled for this run only so
  loopback tests can bind sockets (the Linux workspace sandbox forbids all sockets otherwise). Do not
  contact external services; everything needed is local.
- `CARGO_HOME` is a private registry prefetched with `cargo fetch --locked`, located OUTSIDE the
  snapshot (granted as an extra writable root). `CARGO_TARGET_DIR` is `target/review-build`; probe
  crates may use their own target dirs under `target/`. Build with `--locked` (add `--offline` if
  cargo tries the network; probe crates without a lockfile may use `--offline`).
  Toolchains: 1.97.1 (rust-toolchain.toml), 1.85 (`cargo +1.85`), nightly.
- Put probes under `target/probes/`; restore or report any production-source change; the original
  source integrity is checked after the run. Stop any server you start before finishing.
