# github / e5: one provider document field (nearest variant: a response document)

**Provider changelog.** Repository label objects (`GET /repos/{owner}/{repo}/labels` and every
other place a full label object is returned) now include a boolean `archived` field. Label
objects produced before the change, and GitHub's published examples, omit it: absent means
`false`. Labels embedded in issues are unchanged. Nothing else changed.

Public names the hidden check uses:
- `models::Label::archived: bool` (public field), `true` only when the document says so.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
