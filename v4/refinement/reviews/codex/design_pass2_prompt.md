# Independent Concord V4 product/design challenge — PASS 2 (source comparison)

You are the same independent reviewer role (local Codex GPT-6.1 Sol, xhigh). Your own blind pass-1 report is supplied below as `report_pass1.md`; it was written in a separate session that read only the charter, task corpus and contract cases. This pass follows the "Second pass" and "Output" sections of the supplied owner prompt `CODEX_BLIND_DESIGN.md`.

The current working directory is a disposable, independent snapshot of the frozen V4 source (see `.concord-review.json`: `original_commit` = 8f7adc2019f3ee31af087c69286d559bca1c173a). Public docs: `README.md`, `crates/concord/src/lib.rs` (crate guide), `docs/MIGRATION.md`; normative contract `v4/SEMANTICS.md`; prior rationale `v4/DECISIONS.md`; tests `crates/concord/tests/`, examples `examples/`.

Execution environment for this run:
- Sandbox: workspace-write confined to this snapshot, with network access enabled for this run only so loopback tests can bind sockets. Do not contact external services; everything you need is local.
- `CARGO_HOME` points to a private, prefetched registry inside `target/`; `CARGO_TARGET_DIR` is `target/review-build`. Use `--locked` (and `--offline` if a command tries to reach the network). Build fresh from this source; do not use binaries from anywhere else.
- You may add probe files/crates inside this snapshot (e.g. a scratch downstream crate under `target/probes/` or extra tests) to show real generated signatures, diagnostics and wire behavior. Report each probe and leave production source unchanged (or report the exact diff).

Context: the coordinator has already reproduced and fixed, on a separate branch, three baseline leads — L1 (literal header names were validated only by characters, then `HeaderName::from_static` panicked at runtime for a name longer than http's 65,535-byte limit), L2 (a test and SEMANTICS asserted wire order between different header names, which `HeaderMap` does not promise) and L5 (a test depended on DNS). You may confirm or challenge them briefly, but spend this pass on product/design.

The owner's consequential decision list is supplied as `DECISION_BOARD.md`. Treat it as the agenda: for each row, compare your pass-1 proposal with the implemented control, and say whether a change would pay for itself on concrete tasks — or reject it. Current V4 is a strong control candidate: do not demand a rewrite or novelty. Distinguish defects, incoherent contracts, ergonomic trade-offs, missing evidence and optional improvements, with source locations (file:line) and minimal examples. Propose the smallest discriminating experiment for each important open question.

Your final message is saved verbatim as the pass-2 report. Mark what you ran (commands, outputs) versus what you only read.
