# V4.3 candidate — developer-experience implementation: status

Label: **V4.3 candidate** (not a release; no tag, no publication). The owner authorized merging
PR #2 into `main` in the 4.4 handoff (4 November 2026), after the Windows CI failure below is
resolved and verified; the integration receipt is recorded under "Integration".

## Identities

| What | Value |
|---|---|
| Repository | `gqsnt/concord_api` |
| Accepted V4.2 implementation | `ed6703a29d6f798f13e06379d488feeac33410f4` |
| V4.2 release receipt (`concord-v4.2`) | `85c50024f78a75a12570db154bb17a211d2eb673` |
| V4.2 merge, annotated tag `v4.2.0` target | `b2ba34199b4221906c49d2ed5eb60bd85dca3fcd` (tag object `cf6743b210b1132ca9644ceba64c264f3fb73176`) |
| **Base of this work** (`origin/main` at start) | `5ed600b766fa274acbced0b006e70dcd171fc2bd` — records-only successor of `b2ba341` (changes only `v4/release-4.2/*`) |
| Base CI (exact SHA) | run [41851008386](https://github.com/gqsnt/concord_api/actions/runs/41851008386): success |
| Implementation branch | `concord-v4.3-dx-implementation` |
| Generated-input experiment branch | `concord-v4.3-dx-generated-inputs`, tip `9e4c0efe3896223c8a29e366c5c47cd070a70c20` (G1: evaluated, not adopted) |
| **Final implementation SHA** | `a6fb99843830f46680e77132eb61aa6c67c47497` (verified; see VERIFICATION.md) |
| Records-only successor | the commit adding this line: only `v4/dx-4.3/` changes after `a6fb998` |
| Draft PR | [gqsnt/concord_api#2](https://github.com/gqsnt/concord_api/pull/2) |
| Records-only successor head | `d4ebd34e88a4698f9e936b53884ba50106643adb` — its PR run [41904216870](https://github.com/gqsnt/concord_api/actions/runs/41904216870) **failed** on Windows (job 126834420624, fixture bind WSAENOBUFS); its push run [41904212230](https://github.com/gqsnt/concord_api/actions/runs/41904212230) passed. See CI_FAILURE_RESOLUTION.md |
| CI diagnostics | `7eefdeedcc1ee0646525b6e02ac3f1ba0e02b90b` — socket-pressure snapshots in the gate (diagnostic only) |
| CI correction | `2cdecd30ae526c9c89e28ebcb59965e6091c9305` (Riot fixture lifecycle control, records); its local gate passed (37/37, nightly 16/0) |
| CI review fixes | `f9d8d2d6f33b1e89c387670990d74e5471a2f34e`: local gate 37/37 and nightly 16/0; CI runs 41927326927 and 41927331498 green on Linux, native Windows (two runners) and nightly (16/0) |
| CI correction head | the commit after `f9d8d2d`: re-check fix (snapshot budget per snapshot) |

The campaign's B0 trial sources (`FINAL_EVIDENCE.zip`, the handoff's `reference/campaign/`) are
historical evidence of an older source. Nothing here relabels them as tests of this branch; every
claim below is credited only to commands run on this branch's commits.

## Toolchain and gates

- Toolchain pin and `rust-version`: 1.99.1 (`rust-toolchain.toml`, `Cargo.toml`); local `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Linux x86_64.
- Maintained gate inventory: `scripts/verify.sh` (CI calls it on Linux and native Windows; nightly lane non-blocking).
- Baseline (`5ed600b`, local, before any change): `scripts/verify.sh --package --bench` — 37/37 PASS in 4m35s.
- Final commands: `scripts/verify.sh --package --bench` and `scripts/verify.sh --nightly-only` on the final commit, plus the branch's CI run.

## Work packages

| WP | Scope | Status | Where |
|---|---|---|---|
| W0 | base, branch, ledger, baseline | done | this file |
| W1 | streaming guidance and example; domain adapters; named workflow; bounded traversal | done, maintained | `examples/native/src/upload.rs`, `examples/app`, guides; DECISIONS S1, D1/W1 |
| W2 | metadata-preserving native-edit recipe; public convenience decision | done, maintained recipe; convenience rejected | `examples/native/src/metadata.rs`; DECISIONS M1 |
| W3 | SDK context boundary; independent outside-endpoint crate; core decision | done, maintained; core primitive rejected | `examples/sdk/src/context.rs`, `examples/extension`; DECISIONS X1 |
| W4 | generated-input ADR; macro prototype; matched evaluation; adoption decision | done: evaluated, not adopted (G1 outcome); prototype retained | branch `concord-v4.3-dx-generated-inputs`; `experiments/generated-inputs/` |
| W5 | integration, review, verification, delivery | done | REVIEW_RESOLUTIONS.md, VERIFICATION.md, delivery outside the repository |

Helper branches merged into the implementation branch: `dx43/native` (`e73838e`, `d226b3f`),
`dx43/app` (`4c01954`), `dx43/ext` (`cf3c9fc` = frozen SDK context, `b077e7e`).

## Shared example infrastructure

- `examples/fixture` — wire-recording loopback server (content-length and chunked request
  bodies) and deterministic fake Catalog/Ledger services; a dev-dependency only.
- `examples/sdk` — the Catalog and Ledger declarations the W1/W3/W4 examples share.

## Completion

Current candidate: the CI correction head above. Its exact-commit verification (local gate,
Linux, native Windows, nightly) is **pending** and will be recorded with the integration receipt
under "Integration"; VERIFICATION.md covers earlier commits only. The failed run on `d4ebd34` remains on record
(CI_FAILURE_RESOLUTION.md); it is not relabelled as a pass.

Historical status: **COMPLETE_VERIFIED** for `a6fb998`: every work package is implemented or decided; the
maintained gate passed locally and in CI on Linux and native Windows on that exact commit, and the
nightly conditional-`Clone` step passed (read from its log). Conditional items: generated input
records, metadata convenience in core and a core context primitive were each evaluated and not
adopted (DECISIONS.md). No release action was taken: no `main` merge, tag or publication.

## Integration

Pending: recorded after the merge of PR #2 (merge commit, parents, tree, exact-commit CI on Linux,
native Windows and nightly).
