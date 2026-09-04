# Gate 3 and optional-control decision record

## Precommitted rules

Gate 3 passes only with at least three unfamiliar Rust developers and the
thresholds in `rubric.md`. A relevant non-author reviewer must also inspect
generated Rustdoc and a parent representation semantic diff.

Keep `with_x(T)` only if Variant W improves median correctness-adjusted task
time for tasks 1–3 by at least 15%, produces no repeated high-confidence
outer-presence misconception, and does not reduce generated-error diagnosis
correctness. Mixed, underpowered, or unavailable evidence selects Variant E.

## Candidate decision

Human evidence is unavailable, so ADR 0012 selects Variant E for the candidate:
`set_x(Option<T>)`, `omit_x()`, and initialized-only `reset_x()`. This follows
the release mandate's conservative rule and removes a provisional API. It does
not close Gate 3.

## Release consequence

Until `results.md` contains qualifying human evidence and independent Rustdoc/
diff review, the release branch may be clean and otherwise verified but must
remain on an RC version and must not be merged as stable v2.1.0 into `main`.
