# Performance

Pending: measured on the final candidate after the reviews (an idle machine is required; the
review and trial runs build code). The measurement script is the work directory's
tools/perf_modular.sh: allocation counts and Criterion (`686cd24` vs the candidate), the
`plan_scaling` group (H14), compile time (suites for `686cd24`, the split `dc7f134`, the candidate,
the candidate with native keys), rebuilding the Concord crates, and R5 expansion before/after
(`3a836e7` vs `a4b465a`).
