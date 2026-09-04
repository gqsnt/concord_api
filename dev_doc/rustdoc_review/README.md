# Generated Rustdoc and semantic-diff review packet

This packet closes the human portion of Gate 10 only when a relevant reviewer
other than the primary implementer completes it. Do not replace that reviewer
with a self-review or infer a result from green mechanical checks.

## Preparation

1. Record the exact candidate commit in `results.md`.
2. Run `cargo doc -p concord-examples --all-features --no-deps --locked`.
3. Open the generated `RiotApi` client, one inherited endpoint, the
   `status_v4` nested override, and the explicit-fence fixture Rustdoc.
4. Generate the checked-in before/after parent representation snapshots and run
   `cargo run -p concord-xtask -- semantic-diff <before> <after>`.
5. Give the reviewer `tasks.md` without an implementation walkthrough.

The reviewer may use normal repository search and public docs. Record every
file opened and every correction; assistance after a task starts counts as a
correction.
