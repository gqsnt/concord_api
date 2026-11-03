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