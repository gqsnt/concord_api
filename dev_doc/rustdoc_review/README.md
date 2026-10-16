# Gate 10 independent Rustdoc and semantic-diff review packet

**Status:** v3 protocol, revised 2026-10-16. Human review has not run.
The structured implementer review is recorded separately and cannot fill
human result rows. This packet uses `concord-xtask contract diff` over
`concord.contract/3` snapshots. The v3 lifecycle is owned by
[`../release_gate_status.md`](../release_gate_status.md).

This packet addresses only the human documentation portion of Gate 10 for
public freeze; it is not a local beta-integration gate. It requires at least
one consenting human who understands Rust library usage and
generated documentation, is not the primary implementer, and has not been
trained with the complete Gate 3 packet before this first-contact review. AI or
implementer review does not count.

## Preparation

1. Check out the exact candidate, record its full SHA and platform in
   `results.md`, and build warning-denied public documentation.
2. Generate the maintained example Rustdoc with
   `cargo doc -p concord-examples --all-features --no-deps --locked`.
3. Export a compiled maintained client with
   `cargo run --locked -p concord-xtask --features examples -- contract export --target catalog --output BEFORE.json`.
   Apply one parent representation change in an isolated review checkout,
   export `AFTER.json` with the same command and run
   `cargo run --locked -p concord-xtask -- contract diff BEFORE.json AFTER.json`.
   Record the exact change and native exit status. A nonzero comparison result
   is expected only when the documented command reports a real difference.
4. Give the reviewer only public/generated documentation, normal entry points,
   contract-diff output, and `tasks.md`. Do not provide a macro-source tour or
   this review rubric.
5. Record time and navigation path to every answer, public pages/types opened,
   searches, incorrect interpretations, confidence, corrections, and whether
   implementation source was required.
6. Classify each problem as Critical, Major, or Minor. Fix every Critical and
   release-relevant Major problem, regenerate the exact docs/diff, and obtain a
   targeted independent rereview before passing.

The reviewer may subsequently join Gate 3, but trained Gate 3 understanding
cannot be counted retroactively as first-contact documentation evidence.
