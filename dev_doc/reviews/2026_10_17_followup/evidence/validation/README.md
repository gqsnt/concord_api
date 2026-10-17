# Local follow-up validation

The [index](index.json) pins eleven native check receipts and logs, copied
without altering their bytes. Each receipt records its actual command,
working checkout, source state, start/end time, process duration, exit code,
and raw-log digest. Absolute paths identify the original local execution;
they are not portable prerequisites.

These checks used Rust 1.97.1 and locked dependencies. The receipts retain
dirty development states; they do not identify those states as a clean
committed release. Native durations include compilation and Cargo lock
waiting. The complete pushed-source platform result belongs to that commit's
CI run.

| Check | Result |
|---|---|
| `xtask-followup-tests` | 63 unit tests and 6 CLI tests passed |
| `macro-docs-tests` | 45 macro unit tests passed |
| `user-exercises` | 8 new public-path tests passed with only `json,ndjson` enabled |
| `license-policy` | Repository policy, including project license metadata and packaged-text source copies, passed |
| `fresh-rustdoc` | Warning-denied workspace Rustdoc generated before the targeted documentation corrections |
| `doc-audit-followup` | Current endpoint documentation generated and its provenance assertions passed |
| `redaction-rustdoc` | Current core documentation generated with warnings denied |
| `facade-rustdoc` | Current facade re-export documentation regenerated with warnings denied |
| `followup-clippy` | Full workspace, all targets, all features passed with warnings denied |
| `followup-docs-check-final` | Compiled excerpts, classification, anchors, links and current-language checks passed |
| `followup-format-final` | Workspace formatting passed |

The first integrated docs check found two unanchored illustrative fragments
inside the raw checked-answer record. That record was retained byte-for-byte
as [plain text](../dx/checked-answers.txt), with its artifact name corrected in
the evidence index. The documentation checker remained enforced and the
subsequent check passed.

CSV and inspector old-code failure controls and corrected native runs are
retained in the [runtime evidence](../runtime/README.md). The separate
[Rustdoc evidence](../rustdoc/index.json) retains the original and corrected
semantic diff and the independent reread of generated public pages.

The subsequent [analyzer integration record](analyzer_feature_gate.json)
retains the actual complete-toolchain reproduction of the new test target's
unavailable `NdJson` import, a successful full diagnostic scan after matching
its source gate to Cargo's required features, and a second successful
eight-test run. A preliminary local run without `rust-src` is explicitly
excluded from product evidence. The raw analyzer logs contain terminal
progress output; their hashes and exact hard diagnostics are recorded, while
the original receipts and the eight-test log are retained beside this index.

## Raw evidence and Git whitespace checks

The clean-tree `release-audit` on `622147814c71d9f7107596d58b5325757c3aff22`
reached `git diff --check main HEAD` and reported six captured logs with a final
blank line and thirteen single-space empty-context lines in four unified
patches. Those are original process output and patch syntax. The first local
attempt could not resolve `main`; it was an environment setup result. The real
unchanged remote main was fetched and its local reference established before
the whitespace finding was reproduced.

The root `.gitattributes` preserves captured `.log` and `.patch` bytes in this
evidence subtree with `-text`. Exactly six log paths use
`whitespace=-blank-at-eof`; exactly four patch paths use
`whitespace=-blank-at-eol`. Other whitespace checks remain active. No captured
file or recorded digest was rewritten to make the audit pass. Final audit and
CI conclusions belong to the delivered commit's verification record.
