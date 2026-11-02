# Campaign status — native headers and formats

Branch `concord-v4-native-repr` (worktree `/home/canarit/projects/concord_api_v4`), from
`a915c8d` on `concord-v4-refine`. Pushed once for CI on the owner's authorization (`6bc5032`); not
merged. Preserved unchanged: `concord-v4-refine`
(`a915c8d`), `concord-v4` (`8f7adc2`), V3 (`concord-v3`, tag `concord-v3-preserved`), the alpha tag
`concord-v4-alpha1-baseline`. Work directories (snapshots, worktrees of the side branches, raw
measurements) are under `/home/canarit/projects/concord_v4_native_work/`.

Distinctions used below: **implemented** (on the branch), **tested** (an executable test or
measurement on this branch), **reviewed** (an independent Codex review of that code), **accepted**
(reviewed with every finding fixed or dispositioned, at a recorded revision).

| Step | State | Where |
|---|---|---|
| Baseline frozen; header limitation reproduced | done | snapshot `baseline-a915c8d`; `evidence/baseline_headervalue.txt` |
| Criterion suite (replaces the `Instant` harness) and baseline | done | `benches/criterion` (`native-repr-bench`, merged `93cf892`); baseline run of `a915c8d` |
| Tracks: H (headers), RA (owner defaults), RB (in-place formats), P (Criterion), T (multipart, Clone) | done | `evidence/tracks/` |
| Codex design challenge, blind and on the selected design | done | `reviews/codex/design_pass{1,2}_*`; `REVIEW_TRIAGE.md` |
| Decisions and contract | done | `DECISIONS.md` (N01–N12), `CONTRACT.md` |
| Native header values (N01–N05) | implemented, tested | `76154bc`; `crates/concord/tests/native_headers.rs`, UI cases |
| Formats (N06–N08) | implemented, tested | `71d872d`…`6b7fca0` (Track I); `crates/concord/tests/formats.rs`, `forms.rs`, UI cases |
| Placement-note deduplication (N09) | implemented, tested, measured | `01774a9`; `emit::tests::placement_notes_are_distinct_in_first_seen_order` |
| Conditional `Clone` checks (N10), multipart recipe (N11) | implemented, tested | `checks/clone-compat`, `examples/multipart` (merged `d482a0f`) |
| Docs findings (arity advice, native `Debug` wording) | fixed | `de957c3` |
| Performance (runtime, allocations, compile time) | measured | `PERF.md` |
| Release preparation (package contents, packaged-source consumer, rendered rustdoc) | done | `6e60ba2`, `34bc6b6`, `2c4ce3b`; `scripts/package-check.sh` |
| Fresh-use trial (agent simulation) | done; doc fixes applied | `evidence/fresh_use/`; `2c4ce3b` |
| Full verification of the frozen candidate | done: 22/22 on `66a4478`, `1174d5d`, `d591fe7`, `d6b209f`, `1d9ffe7` | `evidence/verify_*.txt` |
| Codex executable reviews | `66a4478` REVISE (F1–F4) → fixed; `1174d5d`: F1, F2, F4 resolved, F3a found → fixed; `d591fe7`: F3a resolved, F3b–F3d found → fixed; `d6b209f`: F3–F3d resolved, F3e found → fixed; `1d9ffe7`: F3e resolved, ACCEPT, no review pending | `REVIEW_TRIAGE.md` |
| Acceptance | Linux x86_64 at `1d9ffe7` | `FINAL.md` |
| Windows (CI) | run `41459582303` on `6bc5032` (code `1d9ffe7`): all four jobs pass; `test (windows-latest)` runs fmt, clippy, 537 workspace tests, feature lanes, feature isolation, rustdoc; MSRV, nightly and package lanes are not run on Windows | `evidence/ci_41459582303.txt` |
| macOS | NOT_RUN (not a requirement) | — |
