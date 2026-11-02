# Campaign status — native headers and formats

Branch `concord-v4-native-repr` (worktree `/home/canarit/projects/concord_api_v4`), from
`a915c8d` on `concord-v4-refine`. Not pushed, not merged. Preserved unchanged: `concord-v4-refine`
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
| Full verification of the frozen candidate | pending | `FINAL.md` |
| Codex executable review of the candidate | pending | `REVIEW_TRIAGE.md` |
| Windows (CI) | NOT_RUN for this campaign: a push was not authorized; the earlier green Windows run (`41379941540`) is of `a40788c` and is baseline evidence only | — |
| macOS | NOT_RUN (not a requirement) | — |
