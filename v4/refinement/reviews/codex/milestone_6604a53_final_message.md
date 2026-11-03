Verified effective settings: **gpt-6.1-sol, xhigh, execute mode**.

- Original: `6604a53be4e4c92d4e97ae2b71925902565e31fd`
- Snapshot: `a3b5407f92ac7d9b7f6cfd5cdf59de0ad0baec1f`

Four confirmed findings:

- **F-01 · P2:** Multipart example corrupts backslashes in field names and filenames.
- **F-02 · P3:** Raw-keyword client names produce invalid generated Rust.
- **F-03 · P3:** No-default-feature rustdoc fails on unavailable `Json` links.
- **F-04 · P2:** Migration instructions retain the removed constructor and body model.

Personally executed: fresh locked offline build; 138 existing tests across targeted runs; 102 MSRV runtime tests; eight independent probes including 1,458 oracle preparations; three successful mutation-discrimination checks.

The original workspace UI run had a registry-path normalization mismatch; all 51 UI cases passed in a restored, source-identical relocated copy. Windows and rendered-browser QA remain blocked; current performance measurements were not run.

[Full review, reproductions, corrections, 100-case disposition matrix, and evidence](target/probes/review-evidence/REPORT.md)

All 229 origin files and modes remain unchanged.

**Scoped verdict: REVISE for F-01–F-04; coordinator triage remains pending.**